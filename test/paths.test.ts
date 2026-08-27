import { describe, expect, test } from "bun:test";
import { normalizeWindowsPath, pluginRoot } from "../src/paths";

describe("normalizeWindowsPath", () => {
  test("removes an extended drive-path prefix", () => {
    expect(normalizeWindowsPath(String.raw`\\?\C:\foo`)).toBe(String.raw`C:\foo`);
  });

  test("converts an extended UNC path to an ordinary UNC path", () => {
    expect(normalizeWindowsPath(String.raw`\\?\UNC\server\share\foo`)).toBe(
      String.raw`\\server\share\foo`,
    );
  });

  test("leaves ordinary Windows and Unix paths unchanged", () => {
    expect(normalizeWindowsPath(String.raw`C:\foo`)).toBe(String.raw`C:\foo`);
    expect(normalizeWindowsPath("/home/user/plugin")).toBe("/home/user/plugin");
  });

  test("handles empty and missing values", () => {
    expect(normalizeWindowsPath("")).toBe("");
    expect(normalizeWindowsPath(undefined)).toBeUndefined();
  });

  test("also handles slash-separated extended paths", () => {
    expect(normalizeWindowsPath("//?/C:/foo")).toBe("C:/foo");
    expect(normalizeWindowsPath("//?/UNC/server/share/foo")).toBe("//server/share/foo");
  });
});

describe("pluginRoot", () => {
  test("returns an unset value as undefined and normalizes an extended root", () => {
    const original = process.env.HERDR_PLUGIN_ROOT;
    try {
      delete process.env.HERDR_PLUGIN_ROOT;
      expect(pluginRoot()).toBeUndefined();
      process.env.HERDR_PLUGIN_ROOT = String.raw`\\?\C:\plugin`;
      expect(pluginRoot()).toBe(String.raw`C:\plugin`);
    } finally {
      if (original === undefined) delete process.env.HERDR_PLUGIN_ROOT;
      else process.env.HERDR_PLUGIN_ROOT = original;
    }
  });
});
