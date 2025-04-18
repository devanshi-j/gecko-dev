export const description = `
Converts four normalized floating point values to 8-bit unsigned integers, and then combines them into one u32 value.
Component e[i] of the input is converted to an 8-bit unsigned integer value
⌊ 0.5 + 255 × min(1, max(0, e[i])) ⌋ which is then placed in
bits 8 × i through 8 × i + 7 of the result.
`;

import { makeTestGroup } from '../../../../../../common/framework/test_group.js';
import { AllFeaturesMaxLimitsGPUTest } from '../../../../../gpu_test.js';
import { kValue } from '../../../../../util/constants.js';
import { f32, pack4x8unorm, ScalarValue, u32, vec4, Type } from '../../../../../util/conversion.js';
import { quantizeToF32, vectorF32Range } from '../../../../../util/math.js';
import { Case } from '../../case.js';
import { allInputSources, run } from '../../expression.js';

import { builtin } from './builtin.js';

export const g = makeTestGroup(AllFeaturesMaxLimitsGPUTest);

g.test('pack')
  .specURL('https://www.w3.org/TR/WGSL/#pack-builtin-functions')
  .desc(
    `
@const fn pack4x8unorm(e: vec4<f32>) -> u32
`
  )
  .params(u => u.combine('inputSource', allInputSources))
  .fn(async t => {
    const makeCase = (vals: [number, number, number, number]): Case => {
      const vals_f32 = new Array<ScalarValue>(4) as [
        ScalarValue,
        ScalarValue,
        ScalarValue,
        ScalarValue,
      ];
      for (const idx in vals) {
        vals[idx] = quantizeToF32(vals[idx]);
        vals_f32[idx] = f32(vals[idx]);
      }

      return { input: [vec4(...vals_f32)], expected: u32(pack4x8unorm(...vals)) };
    };

    // Returns a value normalized to [0, 1].
    const normalizeF32 = (n: number): number => {
      return n > 0 ? n / kValue.f32.positive.max : n / kValue.f32.negative.min;
    };

    const cases: Array<Case> = vectorF32Range(4).flatMap(v => {
      return [
        makeCase(v as [number, number, number, number]),
        makeCase(v.map(normalizeF32) as [number, number, number, number]),
      ];
    });

    await run(t, builtin('pack4x8unorm'), [Type.vec4f], Type.u32, t.params, cases);
  });
