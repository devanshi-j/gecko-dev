/*
 *  Copyright (c) 2015 The WebRTC project authors. All Rights Reserved.
 *
 *  Use of this source code is governed by a BSD-style license
 *  that can be found in the LICENSE file in the root of the source
 *  tree. An additional intellectual property rights grant can be found
 *  in the file PATENTS.  All contributing project authors may
 *  be found in the AUTHORS file in the root of the source tree.
 */

#include "modules/remote_bitrate_estimator/transport_sequence_number_feedback_generator.h"

#include <algorithm>
#include <cstdint>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include "api/rtp_headers.h"
#include "api/units/data_rate.h"
#include "api/units/data_size.h"
#include "api/units/time_delta.h"
#include "api/units/timestamp.h"
#include "modules/remote_bitrate_estimator/packet_arrival_map.h"
#include "modules/rtp_rtcp/source/rtcp_packet.h"
#include "modules/rtp_rtcp/source/rtcp_packet/transport_feedback.h"
#include "modules/rtp_rtcp/source/rtp_header_extensions.h"
#include "modules/rtp_rtcp/source/rtp_packet_received.h"
#include "rtc_base/checks.h"
#include "rtc_base/logging.h"
#include "rtc_base/synchronization/mutex.h"

namespace webrtc {
namespace {

constexpr TimeDelta kBackWindow = TimeDelta::Millis(500);
constexpr TimeDelta kMinInterval = TimeDelta::Millis(50);
constexpr TimeDelta kMaxInterval = TimeDelta::Millis(250);
constexpr TimeDelta kDefaultInterval = TimeDelta::Millis(100);

}  // namespace

TransportSequenceNumberFeedbackGenenerator::
    TransportSequenceNumberFeedbackGenenerator(RtcpSender feedback_sender)
    : feedback_sender_(std::move(feedback_sender)),
      last_process_time_(Timestamp::MinusInfinity()),
      media_ssrc_(0),
      feedback_packet_count_(0),
      send_interval_(kDefaultInterval),
      send_periodic_feedback_(true) {
  RTC_LOG(LS_INFO)
      << "Maximum interval between transport feedback RTCP messages: "
      << kMaxInterval;
}

TransportSequenceNumberFeedbackGenenerator::
    ~TransportSequenceNumberFeedbackGenenerator() {}

void TransportSequenceNumberFeedbackGenenerator::MaybeCullOldPackets(
    int64_t sequence_number,
    Timestamp arrival_time) {
  if (periodic_window_start_seq_ >=
          packet_arrival_times_.end_sequence_number() &&
      arrival_time - Timestamp::Zero() >= kBackWindow) {
    // Start new feedback packet, cull old packets.
    packet_arrival_times_.RemoveOldPackets(sequence_number,
                                           arrival_time - kBackWindow);
  }
}

void TransportSequenceNumberFeedbackGenenerator::OnReceivedPacket(
    const RtpPacketReceived& packet) {
  if (packet.arrival_time().IsInfinite()) {
    RTC_LOG(LS_WARNING) << "Arrival time not set.";
    return;
  }

  uint16_t seqnum = 0;
  std::optional<FeedbackRequest> feedback_request;
  if (!packet.GetExtension<TransportSequenceNumber>(&seqnum) &&
      !packet.GetExtension<TransportSequenceNumberV2>(&seqnum,
                                                      &feedback_request)) {
    // This function expected to be called only for packets that have
    // TransportSequenceNumber rtp header extension, however malformed RTP
    // packet may contain unparsable TransportSequenceNumber.
    RTC_DCHECK(packet.HasExtension<TransportSequenceNumber>() ||
               packet.HasExtension<TransportSequenceNumberV2>())
        << " Expected transport sequence number.";
    return;
  }

  MutexLock lock(&lock_);
  send_periodic_feedback_ = packet.HasExtension<TransportSequenceNumber>();

  media_ssrc_ = packet.Ssrc();
  int64_t seq = unwrapper_.Unwrap(seqnum);

  if (send_periodic_feedback_) {
    MaybeCullOldPackets(seq, packet.arrival_time());

    if (!periodic_window_start_seq_ || seq < *periodic_window_start_seq_) {
      periodic_window_start_seq_ = seq;
    }
  }

  // We are only interested in the first time a packet is received.
  if (packet_arrival_times_.has_received(seq)) {
    return;
  }

  packet_arrival_times_.AddPacket(seq, packet.arrival_time());

  // Limit the range of sequence numbers to send feedback for.
  if (periodic_window_start_seq_ <
      packet_arrival_times_.begin_sequence_number()) {
    periodic_window_start_seq_ = packet_arrival_times_.begin_sequence_number();
  }

  if (feedback_request.has_value()) {
    // Send feedback packet immediately.
    SendFeedbackOnRequest(seq, *feedback_request);
  }
}

TimeDelta TransportSequenceNumberFeedbackGenenerator::Process(Timestamp now) {
  MutexLock lock(&lock_);
  if (!send_periodic_feedback_) {
    // If TransportSequenceNumberV2 has been received in one packet,
    // PeriodicFeedback is disabled for the rest of the call.
    return TimeDelta::PlusInfinity();
  }
  Timestamp next_process_time = last_process_time_ + send_interval_;
  if (now >= next_process_time) {
    last_process_time_ = now;
    SendPeriodicFeedbacks();
    return send_interval_;
  }

  return next_process_time - now;
}

void TransportSequenceNumberFeedbackGenenerator::OnSendBandwidthEstimateChanged(
    DataRate estimate) {
  // TwccReportSize = Ipv4(20B) + UDP(8B) + SRTP(10B) +
  // AverageTwccReport(30B)
  // TwccReport size at 50ms interval is 24 byte.
  // TwccReport size at 250ms interval is 36 byte.
  // AverageTwccReport = (TwccReport(50ms) + TwccReport(250ms)) / 2
  constexpr DataSize kTwccReportSize = DataSize::Bytes(20 + 8 + 10 + 30);
  constexpr DataRate kMinTwccRate = kTwccReportSize / kMaxInterval;

  // Let TWCC reports occupy 5% of total bandwidth.
  DataRate twcc_bitrate = 0.05 * estimate;

  // Check upper send_interval bound by checking bitrate to avoid overflow when
  // dividing by small bitrate, in particular avoid dividing by zero bitrate.
  TimeDelta send_interval =
      twcc_bitrate <= kMinTwccRate
          ? kMaxInterval
          : std::max(kTwccReportSize / twcc_bitrate, kMinInterval);

  MutexLock lock(&lock_);
  send_interval_ = send_interval;
}

void TransportSequenceNumberFeedbackGenenerator::SendPeriodicFeedbacks() {
  // `periodic_window_start_seq_` is the first sequence number to include in
  // the current feedback packet. Some older may still be in the map, in case
  // a reordering happens and we need to retransmit them.
  if (!periodic_window_start_seq_)
    return;

  int64_t packet_arrival_times_end_seq =
      packet_arrival_times_.end_sequence_number();
  while (periodic_window_start_seq_ < packet_arrival_times_end_seq) {
    auto feedback_packet = MaybeBuildFeedbackPacket(
        /*include_timestamps=*/true, *periodic_window_start_seq_,
        packet_arrival_times_end_seq,
        /*is_periodic_update=*/true);

    if (feedback_packet == nullptr) {
      break;
    }

    RTC_DCHECK(feedback_sender_ != nullptr);
    std::vector<std::unique_ptr<rtcp::RtcpPacket>> packets;
    packets.push_back(std::move(feedback_packet));
    feedback_sender_(std::move(packets));
    // Note: Don't erase items from packet_arrival_times_ after sending, in
    // case they need to be re-sent after a reordering. Removal will be
    // handled by OnPacketArrival once packets are too old.
  }
}

void TransportSequenceNumberFeedbackGenenerator::SendFeedbackOnRequest(
    int64_t sequence_number,
    const FeedbackRequest& feedback_request) {
  if (feedback_request.sequence_count == 0) {
    return;
  }

  int64_t first_sequence_number =
      sequence_number - feedback_request.sequence_count + 1;

  auto feedback_packet = MaybeBuildFeedbackPacket(
      feedback_request.include_timestamps, first_sequence_number,
      sequence_number + 1, /*is_periodic_update=*/false);

  // Even though this is called when a packet has just been added,
  // no feedback may be produced when that new packet is too old.
  if (feedback_packet == nullptr) {
    return;
  }

  // Clear up to the first packet that is included in this feedback packet.
  packet_arrival_times_.EraseTo(first_sequence_number);

  RTC_DCHECK(feedback_sender_ != nullptr);
  std::vector<std::unique_ptr<rtcp::RtcpPacket>> packets;
  packets.push_back(std::move(feedback_packet));
  feedback_sender_(std::move(packets));
}

std::unique_ptr<rtcp::TransportFeedback>
TransportSequenceNumberFeedbackGenenerator::MaybeBuildFeedbackPacket(
    bool include_timestamps,
    int64_t begin_sequence_number_inclusive,
    int64_t end_sequence_number_exclusive,
    bool is_periodic_update) {
  RTC_DCHECK_LT(begin_sequence_number_inclusive, end_sequence_number_exclusive);

  int64_t start_seq =
      packet_arrival_times_.clamp(begin_sequence_number_inclusive);

  int64_t end_seq = packet_arrival_times_.clamp(end_sequence_number_exclusive);

  // Create the packet on demand, as it's not certain that there are packets
  // in the range that have been received.
  std::unique_ptr<rtcp::TransportFeedback> feedback_packet;

  int64_t next_sequence_number = begin_sequence_number_inclusive;

  for (int64_t seq = start_seq; seq < end_seq; ++seq) {
    PacketArrivalTimeMap::PacketArrivalTime packet =
        packet_arrival_times_.FindNextAtOrAfter(seq);
    seq = packet.sequence_number;
    if (seq >= end_seq) {
      break;
    }

    if (feedback_packet == nullptr) {
      feedback_packet =
          std::make_unique<rtcp::TransportFeedback>(include_timestamps);
      feedback_packet->SetMediaSsrc(media_ssrc_);

      // It should be possible to add `seq` to this new `feedback_packet`,
      // If difference between `seq` and `begin_sequence_number_inclusive`,
      // is too large, discard reporting too old missing packets.
      static constexpr int kMaxMissingSequenceNumbers = 0x7FFE;
      int64_t base_sequence_number = std::max(begin_sequence_number_inclusive,
                                              seq - kMaxMissingSequenceNumbers);

      // Base sequence number is the expected first sequence number. This is
      // known, but we might not have actually received it, so the base time
      // shall be the time of the first received packet in the feedback.
      feedback_packet->SetBase(static_cast<uint16_t>(base_sequence_number),
                               packet.arrival_time);
      feedback_packet->SetFeedbackSequenceNumber(feedback_packet_count_++);

      if (!feedback_packet->AddReceivedPacket(static_cast<uint16_t>(seq),
                                              packet.arrival_time)) {
        // Could not add a single received packet to the feedback.
        RTC_DCHECK_NOTREACHED()
            << "Failed to create an RTCP transport feedback with base sequence "
               "number "
            << base_sequence_number << " and 1st received " << seq;
        periodic_window_start_seq_ = seq;
        return nullptr;
      }
    } else {
      if (!feedback_packet->AddReceivedPacket(static_cast<uint16_t>(seq),
                                              packet.arrival_time)) {
        // Could not add timestamp, feedback packet might be full. Return and
        // try again with a fresh packet.
        break;
      }
    }

    next_sequence_number = seq + 1;
  }
  if (is_periodic_update) {
    periodic_window_start_seq_ = next_sequence_number;
  }
  return feedback_packet;
}

}  // namespace webrtc
