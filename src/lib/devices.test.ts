// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
import assert from "node:assert/strict";
import { test } from "node:test";
import { unpairedNearby } from "./devices.ts";

const heard = [
  { deviceId: "kitchen", hosts: ["192.168.1.30"] },
  { deviceId: "study", hosts: ["192.168.1.31"] },
];

test("a paired device never shows under Nearby, whatever its state", () => {
  for (const state of ["online", "offline", "pending", "failed"]) {
    assert.deepEqual(unpairedNearby(heard, [{ deviceId: "kitchen", state }]).map((item) => item.deviceId), ["study"]);
  }
});

test("after Remove the device can be paired again from Nearby", () => {
  assert.deepEqual(unpairedNearby(heard, []).map((item) => item.deviceId), ["kitchen", "study"]);
});

test("devices are matched by id, not by address", () => {
  const moved = [{ deviceId: "kitchen", hosts: ["10.0.0.9"] }, { deviceId: "other", hosts: ["192.168.1.30"] }];
  assert.deepEqual(unpairedNearby(moved, [{ deviceId: "kitchen" }]).map((item) => item.deviceId), ["other"]);
});
