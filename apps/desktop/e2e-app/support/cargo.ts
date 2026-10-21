import { execFileSync } from "node:child_process";

/**
 * Where cargo builds the workspace in `dir`: its `target` folder, unless
 * CARGO_TARGET_DIR or a cargo config of the machine's names another, as
 * one folder shared by every checkout does.
 */
export function targetDir(dir: string): string {
  const metadata = execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], {
    cwd: dir,
    encoding: "utf8",
  });
  return (JSON.parse(metadata) as { target_directory: string }).target_directory;
}
