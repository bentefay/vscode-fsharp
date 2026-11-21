'use strict';

// Shared inputs for the native editor suite and explicitly labelled adapter
// harness. Marker helpers locate source text; tests never use spelling as proof.
exports.files = {
  'Ordered.fsproj': '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net10.0</TargetFramework></PropertyGroup><ItemGroup><Compile Include="Zulu.fs" /><Compile Include="Alpha.fs" /><Compile Include="Members.fs" /><Compile Include="Main.fs" /></ItemGroup></Project>\n',
  'Zulu.fs': 'module LibraryA\nlet value = 1\n',
  'Alpha.fs': 'module LibraryB\nlet value = 2\n',
  'Members.fs': 'module Members\ntype Person = { Name: string }\ntype Company = { Name: string }\nlet read (x: Person) = x.Name\nlet other (x: Company) = x.Name\n',
  'Main.fs': 'module Consumer\nopen LibraryA\nopen LibraryB\nlet result = value\nlet first input =\n    let shared = input + result\n    shared\nlet second input =\n    let shared = input + 1\n    shared\nlet unknown (receiver: obj) = receiver.NotKnown\nlet externalValue = ExternalLibrary.answer\n',
};

exports.catalog = assemblyPath => ({
  schemaVersion: 1,
  symbols: [{ id: 'fixture:ExternalLibrary.answer', assemblyName: 'ExternalLibrary', assemblyPath, fullName: 'ExternalLibrary.answer', qualifiedNames: ['ExternalLibrary.answer'], name: 'answer', kind: 'value', signature: 'val answer : int', documentation: 'The answer from a library.', xmlDocId: 'P:ExternalLibrary.answer', isExternal: true }],
  completeness: 'exact',
});
