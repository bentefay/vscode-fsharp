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
    const snapshots = scenario.operation === 'projectTree' ? [] : [{query: scenario.query, files: {...fixture.files, ...(scenario.edit ? {[scenario.edit.file]: scenario.edit.text} : {})}}];
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
    const referenceCase = ['references', 'referenceFreshness'].includes(scenario.operation);
    assert.ok((scenario.operation === 'projectTree' ? ['ordered-tree'] : scenario.operation === 'hover' ? ['suppressed', 'external', 'unavailable'] : referenceCase ? ['exact', 'incomplete'] : ['exact', 'candidates', 'unavailable']).includes(scenario.expected.kind));
    if (scenario.operation === 'hover' && scenario.expected.kind !== 'external') { assert.equal(scenario.expected.ownership, scenario.expected.kind === 'suppressed' ? 'workspace' : 'unknown'); assert.equal(scenario.expected.completeness, scenario.expected.kind === 'suppressed' ? 'exact' : 'unavailable'); }
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
      ...(['references', 'referenceFreshness'].includes(scenario.operation) ? [scenario.oracle.declaration, ...scenario.oracle.uses, ...(scenario.oracle.excludedHomonyms || []), ...(scenario.oracle.unrelatedSameNameOccurrences || [])].map(location => ({location, files: edited})) : []),
      ...(expected.target ? [{location: expected.target, files: edited}] : []),
      ...(expected.forbidden || []).map(location => ({location, files: fixture.files})),
      ...(scenario.edit ? [{location: scenario.edit.baseline.expected.target, files: fixture.files}] : [])];
    for (const {location, files} of locations) {
      const { start, end } = location.range;
      assert.equal(start.line, end.line);
      const line = files[location.file].split('\n')[start.line];
      assertLiteralSpan(line, start.character, end.character, location.text, scenario.id);
    }
    if (expected.kind === 'exact' && !['references', 'referenceFreshness'].includes(scenario.operation)) assert.ok(expected.target);
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

// New UI routes do not rewrite any previously accepted or expected-failure contract.
test('prior seventeen cases and nine fixtures remain frozen', () => {
  const prior = {fixtures: manifest.fixtures.slice(0, 9), cases: manifest.cases.slice(0, 17)};
  assert.equal(createHash('sha256').update(JSON.stringify(prior)).digest('hex'),
    '222a4bf08d39ce94421a95e8e8b65be361eb9c590aa196a19023f7ae462a92f2');
});

test('tree leaves follow literal compile order and retain physical linked targets', () => {
  for (const scenario of manifest.cases.filter(c => c.operation === 'projectTree')) {
    const fixture = manifest.fixtures.find(f => f.id === scenario.fixture);
    const project = scenario.expected.tree.project;
    assert.equal(project, fixture.project);
    const xml = fixture.files[project];
    const items = [...xml.matchAll(/<Compile Include="([^"]+)"\s*(?:\/>|>([\s\S]*?)<\/Compile>)/g)];
    const expected = items.map(item => {
      const link = item[2]?.match(/<Link>([^<]+)<\/Link>/)?.[1];
      return {file: path.posix.normalize(path.posix.join(path.posix.dirname(project), item[1])), displayPath: (link || item[1]).split('/')};
    });
    assert.deepEqual(scenario.expected.tree.leaves, expected);
    for (const leaf of expected) assert.ok(fixture.files[leaf.file]);
    assert.notDeepEqual(expected.map(x => x.file), expected.map(x => x.file).sort());
  }
});

test('opt-in prepared hover contract pins genuine assets and literal queries', () => {
  const prepared = JSON.parse(fs.readFileSync(path.join(__dirname, 'prepared-hover-manifest.json'), 'utf8'));
  for (const scenario of prepared.cases) {
    const fixture = prepared.fixtures.find(f => f.id === scenario.fixture);
    assert.equal(fixture.externalAsset, 'prepared-library-hover-v1');
    assert.equal(scenario.operation, 'hover');
    assert.ok(['external', 'suppressed'].includes(scenario.expected.kind));
    assert.equal(scenario.expected.ownership, scenario.expected.kind === 'external' ? 'external' : 'workspace');
    if (scenario.expected.kind === 'external') assert.ok(scenario.expected.contains.length >= 4);
    else assert.equal(scenario.expected.completeness, 'exact');
    for (const hash of Object.values(fixture.assetPins)) assert.match(hash, /^[a-f0-9]{64}$/);
    if (scenario.expected.kind === 'external') for (const hash of [scenario.oracle.assemblySha256, scenario.oracle.xmlSha256]) assert.match(hash, /^[a-f0-9]{64}$/);
    const q = scenario.query, line = fixture.files[q.file].split('\n')[q.line];
    assertLiteralSpan(line, q.character, q.character + q.token.length, q.token, scenario.id);
    if (scenario.expected.kind === 'external') assert.ok(scenario.oracle.xmlDocId);
  }
});

// Scope-rule additions must not rewrite earlier accepted contracts.
test('prior twenty cases and eleven fixtures remain frozen', () => {
  const prior = {fixtures: manifest.fixtures.slice(0, 11), cases: manifest.cases.slice(0, 20)};
  assert.equal(createHash('sha256').update(JSON.stringify(prior)).digest('hex'),
    'f6343f11673432b33912ab20729ce7fdab3132ce1ee36301856f1cbc07b00d6b');
});

test('qualified union regression remains strict with the unchanged complete fixture', () => {
  const regression = JSON.parse(fs.readFileSync(path.join(__dirname, 'union-definition-regression.json'), 'utf8'));
  assert.equal(regression.schemaVersion, 1);
  assert.equal(regression.provenance.firstExecutedManifestSha256,
    '04ab67f62d45f049e9a6b45a3e1a1c314e5c81c8b2a1f7970dd32044ed19c8cd');
  assert.equal(regression.fixtures.length, 1);
  const fixture = manifest.fixtures.find(f => f.id === 'exact-scope-rules');
  assert.deepEqual(regression.fixtures[0], fixture);
  assert.equal(regression.cases.length, 1);
  const scenario = regression.cases[0];
  assert.equal(scenario.id, 'qualified-union-case-identity');
  assert.equal(scenario.fixture, fixture.id);
  assert.equal(scenario.expected.kind, 'exact');
  assert.equal(scenario.expectedFailure, undefined);
  assert.ok(!manifest.cases.some(c => c.id === scenario.id));
  assert.deepEqual(scenario.query, {file: 'Union.fs', line: 7, character: 22, token: 'Item'});
  assert.deepEqual(scenario.expected.target, {file: 'Union.fs', range: {
    start: {line: 3, character: 6}, end: {line: 3, character: 10}}, text: 'Item'});
  assert.deepEqual(scenario.expected.forbidden, [{file: 'Union.fs', range: {
    start: {line: 6, character: 6}, end: {line: 6, character: 10}}, text: 'Item'}]);
  const lines = fixture.files['Union.fs'].split('\n');
  assert.equal(lines[2], 'type Parcel =');
  assert.equal(lines[5], 'type Other =');
  assertLiteralSpan(lines[scenario.query.line], scenario.query.character,
    scenario.query.character + scenario.query.token.length, scenario.query.token, scenario.id);
  for (const location of [scenario.expected.target, ...scenario.expected.forbidden]) {
    assertLiteralSpan(lines[location.range.start.line], location.range.start.character,
      location.range.end.character, location.text, scenario.id);
  }
});

// Interaction extensions preserve every pre-existing scenario and fixture.
test('all twenty-five prior cases and fixtures remain frozen', () => {
  assert.equal(createHash('sha256').update(JSON.stringify({fixtures:manifest.fixtures,cases:manifest.cases.slice(0,25)})).digest('hex'),'dafd2910175293c8e552b2a5a1d746bdd261e61d168b6d3a44edfaf659f53f31');
});

test('two reference interaction plans have independent current UTF-16 sets and one-line edits', () => {
  const regression=JSON.parse(fs.readFileSync(path.join(__dirname,'reference-currentness-regression.json'),'utf8'));assert.equal(regression.cases.length,2);assert.deepEqual(regression.cases.map(c=>c.id),['references-native-post-edit-repeat','references-possible-post-edit-currentness']);for(const c of regression.cases){assert.equal(c.expectedFailure,undefined);if(c.id==='references-native-post-edit-repeat')assert.ok(!manifest.cases.some(x=>x.id===c.id));else assert.deepEqual(manifest.cases.find(x=>x.id===c.id),c);}for(const f of regression.fixtures)assert.deepEqual(f,manifest.fixtures.find(x=>x.id===f.id));const interactions=regression.cases;assert.equal(interactions.length,2);
  for(const c of interactions){
    const fixture=manifest.fixtures.find(f=>f.id===c.fixture),plan=c.interaction;assert.equal(plan.edit.prefix,'// Reference shift\n');assert.ok(fixture.files[plan.edit.file]);
    assert.equal(plan.repeats,plan.kind==='native-post-edit-repeat'?2:1);
    const files={...fixture.files,[plan.edit.file]:plan.edit.prefix+fixture.files[plan.edit.file]},after=plan.after,q=after.query,start=q.tokenStart??q.character;assertLiteralSpan(files[q.file].split('\n')[q.line],start,start+q.token.length,q.token,c.id);assert.ok(q.character>=start&&q.character<start+q.token.length);
    const old=c.expected.references||c.expected.possibleRequired,newSet=after.expected.references||after.expected.possibleRequired;assert.equal(newSet.length,old.length);
    for(let i=0;i<old.length;i++){const expected=JSON.parse(JSON.stringify(old[i]));if(expected.file===plan.edit.file){expected.range.start.line++;expected.range.end.line++;}assert.deepEqual(newSet[i],expected);const t=newSet[i];assertLiteralSpan(files[t.file].split('\n')[t.range.start.line],t.range.start.character,t.range.end.character,t.text,c.id);}
    const oracle=after.oracle;assert.equal(oracle.knownUsageCount,oracle.uses.length);const authored=[oracle.declaration,...oracle.uses,...(oracle.unrelatedSameNameOccurrences||[])];assert.deepEqual(newSet.map(x=>JSON.stringify(x)).sort(),authored.map(x=>JSON.stringify(x)).sort());
    for(const t of oracle.excludedHomonyms||[]){assertLiteralSpan(files[t.file].split('\n')[t.range.start.line],t.range.start.character,t.range.end.character,t.text,c.id);assert.ok(!newSet.some(x=>JSON.stringify(x)===JSON.stringify(t)));}
    if(plan.kind==='possible-post-edit-command')assert.deepEqual(after.expected.possibleTarget,newSet.find(x=>x.file==='Consumer.fs'));
  }
});
