// Hand-written CSS takes every colour from a token; `od:theme-exempt` marks deliberate diagnostics.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..', '..');

const ROOTS = [
  join(root, 'packages', 'console-ui', 'src'),
  join(root, 'apps', 'console-lab', 'src'),
];

function findCssFiles(dir: string): string[] {
  const found: string[] = [];
  for (const name of readdirSync(dir)) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) {
      found.push(...findCssFiles(full));
    } else if (name.endsWith('.css')) {
      found.push(full);
    }
  }
  return found;
}

const COLOUR = /#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(|\boklch\(/;

function literalColours(css: string): string[] {
  const found: string[] = [];
  let exempt = false;
  css.split('\n').forEach((line, i) => {
    if (line.includes('od:theme-exempt:start')) exempt = true;
    if (line.includes('od:theme-exempt:end')) exempt = false;
    if (!exempt && COLOUR.test(line)) found.push(`  line ${i + 1}: ${line.trim().slice(0, 90)}`);
  });
  return found;
}

const files = ROOTS.flatMap(findCssFiles);

test('every hand-written stylesheet was found', () => {
  assert.ok(files.length >= 4, `expected at least 4 CSS files, found ${files.length}: ${files.join(', ')}`);
});

for (const file of files) {
  test(`no hard-coded colours in ${file.slice(root.length + 1)}`, () => {
    const literals = literalColours(readFileSync(file, 'utf8'));
    assert.deepEqual(literals, [], `hard-coded colours cannot follow the theme:\n${literals.join('\n')}`);
  });
}
