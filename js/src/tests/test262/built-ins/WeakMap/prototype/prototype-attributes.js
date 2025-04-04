// Copyright (C) 2015 the V8 project authors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-weakmap.prototype
description: >
  WeakMap.prototype is not writable, not enumerable and not configurable.
includes: [propertyHelper.js]
---*/

verifyProperty(WeakMap, 'prototype', {
  writable: false,
  enumerable: false,
  configurable: false,
});

reportCompare(0, 0);
