import { describe, expect, it } from "vitest";
import { isAllowed } from "./check-licenses.mjs";

describe("isAllowed（npm 側のライセンスの判定）", () => {
  it("許可された1つのライセンスを通す", () => {
    expect(isAllowed("MIT", "a")).toBe(true);
  });
  it("OR のどれかが許可されていれば通す（括弧つきも）", () => {
    expect(isAllowed("Apache-2.0 OR MIT", "a")).toBe(true);
    expect(isAllowed("(MIT OR GPL-3.0-only)", "a")).toBe(true);
  });
  it("許可にないライセンスは通さない", () => {
    expect(isAllowed("GPL-3.0-only", "a")).toBe(false);
  });
  it("AND や入れ子の括弧を含む式は通さない（監査 P0-03 の反例）", () => {
    expect(isAllowed("MIT AND (Apache-2.0 OR ISC) AND GPL-3.0-only", "a")).toBe(
      false,
    );
    expect(isAllowed("MIT AND ISC", "a")).toBe(false);
  });
  it("MPL-2.0 は例外の部品だけ通す", () => {
    expect(isAllowed("MPL-2.0", "lightningcss")).toBe(true);
    expect(isAllowed("MPL-2.0", "lightningcss-win32-x64-msvc")).toBe(true);
    expect(isAllowed("MPL-2.0", "other")).toBe(false);
  });
});
