import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const { p0TimeProbe } = await import("./commands");

describe("Tauri のコマンドの呼び出し", () => {
  beforeEach(() => invoke.mockReset());

  it("p0TimeProbe は Rust の p0_time_probe を引数なしで呼ぶ", async () => {
    invoke.mockResolvedValue({ ns: 9007199254740991 });
    const t = await p0TimeProbe();
    expect(invoke).toHaveBeenCalledWith("p0_time_probe");
    expect(t.ns).toBe(9007199254740991);
    expect(Number.isSafeInteger(t.ns)).toBe(true);
  });

  it("JSON の上限の値は number で正確に読める（2^53 − 1）", () => {
    expect(JSON.parse('{"ns":9007199254740991}').ns).toBe(
      Number.MAX_SAFE_INTEGER,
    );
  });
});
