'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, 'manifest.json'), 'utf8'));

function assertLiteralSpan(line, start, end, literal, label) {
  assert.ok(Number.isInteger(start) && Number.isInteger(end) && start >= 0 && end > start);
  assert.ok(typeof line === 'string' && end <= line.length, label);
  assert.equal(typeof literal, 'string', 'Author literal destination text for each span');
  assert.equal(line.slice(start, end), literal, label);
}

// These are manifest-integrity checks, never counted as editor cases or language proof.
test('each scenario has a distinct rule, explicit oracle and a real query token', () => {
  assert.equal(manifest.schemaVersion, 1);
  assert.equal(new Set(manifest.cases.map(c => c.id)).size, manifest.cases.length);
  assert.equal(new Set(manifest.cases.map(c => c.ruleId)).size, manifest.cases.length);
  for (const scenario of manifest.cases) {
    const fixture = manifest.fixtures.find(f => f.id === scenario.fixture);
    assert.ok(fixture, scenario.id);
    assert.ok(scenario.oracle.explanation && /^https:\/\//.test(scenario.oracle.source));
    const snapshots = [{query: scenario.query, files: {...fixture.files, ...(scenario.edit ? {[scenario.edit.file]: scenario.edit.text} : {})}}];
    if (scenario.edit) {
      assert.ok(fixture.files[scenario.edit.file] && typeof scenario.edit.text === 'string' && scenario.edit.baseline);
      assert.notEqual(scenario.edit.text, fixture.files[scenario.edit.file]);
      snapshots.push({query: scenario.edit.baseline.query, files: fixture.files});
    }
    for (const {query, files} of snapshots) {
      const tokenStart = query.tokenStart ?? query.character;
      assert.ok(Number.isInteger(tokenStart) && tokenStart >= 0);
      assert.ok(query.character >= tokenStart && query.character < tokenStart + query.token.length, 'Caret must be inside the authored full token');
      assert.equal(files[query.file].split('\n')[query.line].slice(tokenStart, tokenStart + query.token.length), query.token, scenario.id);
    }
    const referenceCase = scenario.operation === 'references';
    assert.ok((referenceCase ? ['exact', 'incomplete'] : ['exact', 'candidates', 'unavailable']).includes(scenario.expected.kind));
    if (referenceCase) {
      assert.ok(['sidebar', 'peek'].includes(scenario.route));
      assert.equal(typeof scenario.declarationIncluded, 'boolean');
    }
    if (scenario.expectedFailure) assert.ok(scenario.expectedFailure.reason && scenario.expectedFailure.issue && scenario.expectedFailure.match, 'Expected failures need an explicit reproduced defect reference');
  }
});
test('fixtures are portable source-only inputs and expected ranges identify nonempty source', () => {
  for (const fixture of manifest.fixtures) {
    assert.ok(fixture.files[fixture.project]);
    if (fixture.workspaceFolder) assert.ok(!path.isAbsolute(fixture.workspaceFolder) && !fixture.workspaceFolder.split(/[\\/]/).includes('..'));
    for (const name of Object.keys(fixture.files)) {
      assert.ok(!path.isAbsolute(name) && !name.split(/[\\/]/).includes('..'), name);
      if (name === 'Directory.Build.props') assert.equal(fixture.files[name], '<Project/>\n', 'Only the exact inert ambient-context fixture is allowed');
      else assert.match(name, /\.(fs|fsi|fsx|fsproj)$/);
    }
  }
  for (const scenario of manifest.cases) {
    const fixture = manifest.fixtures.find(f => f.id === scenario.fixture), expected = scenario.expected;
    const edited = {...fixture.files, ...(scenario.edit ? {[scenario.edit.file]: scenario.edit.text} : {})};
    const locations = [...(expected.required || []).map(location => ({location, files: edited})),
      ...(expected.references || []).map(location => ({location, files: edited})),
      ...(expected.possibleRequired || []).map(location => ({location, files: edited})),
      ...(expected.possibleTarget ? [{location: expected.possibleTarget, files: edited}] : []),
      ...(scenario.operation === 'references' ? [scenario.oracle.declaration, ...scenario.oracle.uses, ...(scenario.oracle.excludedHomonyms || []), ...(scenario.oracle.unrelatedSameNameOccurrences || [])].map(location => ({location, files: edited})) : []),
      ...(expected.target ? [{location: expected.target, files: edited}] : []),
      ...(expected.forbidden || []).map(location => ({location, files: fixture.files})),
      ...(scenario.edit ? [{location: scenario.edit.baseline.expected.target, files: fixture.files}] : [])];
    for (const {location, files} of locations) {
      const { start, end } = location.range;
      assert.equal(start.line, end.line);
      const line = files[location.file].split('\n')[start.line];
      assertLiteralSpan(line, start.character, end.character, location.text, scenario.id);
    }
    if (expected.kind === 'exact' && scenario.operation !== 'references') assert.ok(expected.target);
    if (expected.kind === 'candidates') {
      assert.ok(expected.required.length >= 1);
      assert.ok(expected.required.some(x => JSON.stringify(x) === JSON.stringify(expected.target)));
    }
  }
});

test('reference sets are complete authored UTF-16 sets, with usage counts separate from declarations', () => {
  const keys = locations => locations.map(location => JSON.stringify(location)).sort();
  for (const scenario of manifest.cases.filter(c => c.operation === 'references')) {
    const {expected, oracle} = scenario;
    assert.equal(oracle.knownUsageCount, oracle.uses.length);
    if (expected.kind === 'exact') {
      const all = [...(scenario.declarationIncluded ? [oracle.declaration] : []), ...oracle.uses];
      assert.deepEqual(keys(expected.references), keys(all), scenario.id);
      assert.equal(new Set(keys(expected.references)).size, expected.references.length);
      for (const homonym of oracle.excludedHomonyms) assert.ok(!keys(all).includes(JSON.stringify(homonym)));
      assert.deepEqual(keys(expected.forbidden), keys(oracle.excludedHomonyms));
    } else {
      assert.equal(scenario.showPossible, true);
      assert.equal(expected.possibleSet, 'exact');
      assert.deepEqual(keys(expected.possibleRequired), keys([oracle.declaration, ...oracle.uses, ...oracle.unrelatedSameNameOccurrences]));
      assert.equal(new Set(keys(expected.possibleRequired)).size, expected.possibleRequired.length);
      assert.ok(keys(expected.possibleRequired).includes(JSON.stringify(expected.possibleTarget)));
      assert.equal(expected.references, undefined, 'Incomplete FAR must not masquerade as an exact empty reference set');
    }
  }
});

test('all twelve prior definition cases and seven prior fixtures remain byte-semantically frozen', () => {
  const original = {fixtures: manifest.fixtures.slice(0, 7), cases: manifest.cases.slice(0, 12)};
  assert.equal(createHash('sha256').update(JSON.stringify(original)).digest('hex'),
    '4c22572c91e986175ca157979fdc694f14a1098961267117c67b2a38e53f7925');
});

test('the original six passing scenario and fixture contracts remain frozen', () => {
  // Additive literal/set-strengthening metadata may change; source, queries and golden locations may not.
  const withoutMetadata = value => Array.isArray(value) ? value.map(withoutMetadata) : value && typeof value === 'object'
    ? Object.fromEntries(Object.entries(value).filter(([key]) => !(key === 'text' && value.file && value.range) && !(key === 'candidateSet' && value.kind === 'candidates')).map(([key, item]) => [key, withoutMetadata(item)])) : value;
  const original = withoutMetadata({fixtures: manifest.fixtures.slice(0, 2), cases: manifest.cases.slice(0, 6)});
  assert.equal(createHash('sha256').update(JSON.stringify(original)).digest('hex'),
    '7d7051dd2cca9703c1ad9a704b79769dc588ef9ed94abf508f439d9ed5297549');
});

test('literal spans use UTF-16 coordinates for operators, quoted names and Unicode', () => {
  const source = 'let ``väl ue`` = (++) 😀';
  assertLiteralSpan(source, 4, 14, '``väl ue``');
  assertLiteralSpan(source, 18, 20, '++');
  assertLiteralSpan(source, 22, 24, '😀');
  assert.throws(() => assertLiteralSpan(source, 22, 23, '😀'), 'A partial surrogate is not the authored literal');
});
