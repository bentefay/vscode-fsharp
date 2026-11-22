'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, 'manifest.json'), 'utf8'));

// These are manifest-integrity checks, never counted as editor cases or language proof.
test('each scenario has a distinct rule, explicit oracle and a real query token', () => {
  assert.equal(manifest.schemaVersion, 1);
  assert.equal(new Set(manifest.cases.map(c => c.id)).size, manifest.cases.length);
  assert.equal(new Set(manifest.cases.map(c => c.ruleId)).size, manifest.cases.length);
  for (const scenario of manifest.cases) {
    const fixture = manifest.fixtures.find(f => f.id === scenario.fixture);
    assert.ok(fixture, scenario.id);
    assert.ok(scenario.oracle.explanation && /^https:\/\//.test(scenario.oracle.source));
    const query = scenario.query;
    assert.equal(fixture.files[query.file].split('\n')[query.line].slice(query.character, query.character + query.token.length), query.token, scenario.id);
    assert.ok(['exact', 'candidates', 'unavailable'].includes(scenario.expected.kind));
    if (scenario.expectedFailure) assert.ok(scenario.expectedFailure.reason && scenario.expectedFailure.issue && scenario.expectedFailure.match, 'Expected failures need an explicit reproduced defect reference');
  }
});
test('fixtures are portable source-only inputs and expected ranges identify nonempty source', () => {
  for (const fixture of manifest.fixtures) {
    assert.ok(fixture.files[fixture.project]);
    for (const name of Object.keys(fixture.files)) {
      assert.ok(!path.isAbsolute(name) && !name.split(/[\\/]/).includes('..'), name);
      assert.match(name, /\.(fs|fsi|fsx|fsproj)$/);
    }
  }
  for (const scenario of manifest.cases) {
    const fixture = manifest.fixtures.find(f => f.id === scenario.fixture), expected = scenario.expected;
    const locations = [...(expected.required || []), ...(expected.forbidden || []), ...(expected.target ? [expected.target] : [])];
    for (const location of locations) {
      const { start, end } = location.range;
      assert.equal(start.line, end.line);
      assert.ok(start.character >= 0 && end.character > start.character);
      const line = fixture.files[location.file].split('\n')[start.line];
      assert.ok(line && end.character <= line.length, scenario.id);
      assert.match(line.slice(start.character, end.character), /^[A-Za-z_][A-Za-z0-9_]*$/);
    }
    if (expected.kind === 'exact') assert.ok(expected.target);
    if (expected.kind === 'candidates') {
      assert.ok(expected.required.length >= 2);
      assert.ok(expected.required.some(x => JSON.stringify(x) === JSON.stringify(expected.target)));
    }
  }
});
