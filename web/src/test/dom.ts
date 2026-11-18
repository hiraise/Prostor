// DOM queries for layout tests: `querySelector` answers typed as nullable, tests want a plain
// error over non-null assertions (eslint forbids both `as` and `!` here).
export function need<T>(value: T | null | undefined, what: string): T {
  if (value === null || value === undefined) throw new Error(`missing ${what}`);
  return value;
}
