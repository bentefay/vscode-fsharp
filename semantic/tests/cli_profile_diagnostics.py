#!/usr/bin/env python3
"""Actual task-local CLI checks; simulated Nix branch is explicitly not Nix execution."""
import argparse,hashlib,json,os,subprocess
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]

def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--helper',type=Path,required=True)
    parser.add_argument('--fixture',type=Path,required=True)
    parser.add_argument('--output-dir',type=Path,required=True)
    args=parser.parse_args();out=args.output_dir.resolve();out.mkdir(parents=True,exist_ok=False)
    report={'kind':'independent-cli-profile-diagnostics','actualNixExecution':False,'helper':str(args.helper.resolve()),'helperSha256':sha(args.helper),'checks':[]}
    source_paths=['devenv.nix','devenv.yaml','devenv.lock','nix/dotnet-10.0.400.nix','nix/dotnet-10.0.400.json','nix/toolchain.nix','cli/commands/development.mjs','cli/commands/prepare.mjs','crates/navigator-core/src/environment.rs','crates/navigator-core/src/globalization.rs','crates/navigator-core/src/producer_contract.rs','package.json','.npmrc']
    before={p:sha(ROOT/p) for p in source_paths}
    def save():
        (out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    def run(label,args,changes=None,expected=0):
        command=[str(ROOT/'scripts/with-local-tools'),*map(str,args)]
        result=subprocess.run(command,cwd=ROOT,env={**os.environ,**(changes or {})},text=True,capture_output=True,timeout=120)
        row={'label':label,'command':command,'environmentChanges':changes or {},'exitCode':result.returncode,'stdout':result.stdout,'stderr':result.stderr}
        report['checks'].append(row);save()
        assert result.returncode==expected,row
        return row
    for name in ['cli/commands/development.mjs','cli/commands/prepare.mjs','cli/lib/process.mjs']:
        run('JavaScript syntax: '+name,['node','--check',name])
    actual=run('Actual task-local doctor',['cli/bin/run','doctor'])
    assert '10.0.400' in actual['stdout'] and 'Microsoft.NETCore.App 10.0.11' in actual['stdout']
    assert 'task-local verified tools' in actual['stdout']
    simulated=run('Simulated FN_DEVENV diagnostic with task-local tools; not Nix',['cli/bin/run','doctor'],{'FN_DEVENV':'1'})
    assert 'preparation under Nix is unsupported' in simulated['stderr']
    assert 'Nix build/editor execution is also unverified' in simulated['stderr']
    fixture=json.loads((args.fixture/'manifest.json').read_text())['programs']['positive']
    override=out/'unsupported-locale-path';override.mkdir()
    target=out/'unsupported-preparation'
    rejected=run('Actual helper refuses unsupported locale; diagnostic flag cannot bypass certificate',[
        'cli/bin/run','prepare','--root',fixture['root'],'--out',target,'--helper',args.helper.resolve(),
        '--metrics',out/'unsupported-helper-metrics.json'],{'FN_DEVENV':'1','LOCPATH':str(override)},expected=1)
    assert 'committed Nix toolchain has no validated reusable-preparation profile' in rejected['stderr']
    assert 'LOCPATH' in rejected['stdout']+rejected['stderr'],rejected
    if 'Uncertified ambient/property condition' in rejected['stdout']:
        assert rejected['stdout'].index('LOCPATH') < rejected['stdout'].index('Uncertified ambient/property condition')
    model=json.loads((target/'projects.json').read_text())
    assert any('Unsupported startup selector: LOCPATH' in message for project in model['projects'] for message in project.get('membershipDiagnostics',[]))
    assert not (target/'settings.json').exists()
    assert not (target/'libraries.json').exists() and not (target/'builtins.json').exists()
    metrics=json.loads((out/'unsupported-helper-metrics.json').read_text())
    assert [v['method'] for v in metrics['phases']]==['workspace/evaluate']
    run('TypeScript typecheck',['node','node_modules/typescript/bin/tsc','-p','extension/tsconfig.json','--noEmit'])
    run('TypeScript compile',['npm','run','compile'])
    frontend=run('Frontend transport/virtual-document/adapter contracts',['node','--test','extension/test/transport.test.cjs','extension/test/virtualDocuments.test.cjs','extension/test/adapterContract.test.cjs'])
    assert 'pass 23' in frontend['stdout'] or '# pass 23' in frontend['stdout']
    provenance=json.loads((ROOT/'nix/dotnet-10.0.400.json').read_text())
    assert sha(ROOT/'nix'/provenance['file'])==provenance['sha256']
    assert sha(ROOT/'nix'/provenance['sourceLicenseFile'])==provenance['sourceLicenseSha256']
    archive_evidence=json.loads((ROOT/'nix'/provenance['archiveVerification']).resolve().read_text())
    assert len(archive_evidence['verifiedArtifacts'])==24 and all(v['matches'] for v in archive_evidence['verifiedArtifacts'])
    assert before=={p:sha(ROOT/p) for p in source_paths},'Reviewed sources changed during checks'
    report.update(passed=True,sourceSha256=before,settingsPublished=False,metadataExportStarted=False,
                  manifestIdentityVerified=True,archiveEvidenceRecords=24,frontendTestsPassed=23,
                  limits='Actual protected task-local tools and immutable helper used. FN_DEVENV=1 is only a labeled message-branch simulation. No Nix expression evaluation, realization, shell, build or editor execution by this suite; follow the separate current Nix portability audit. The real unsupported locale control reaches workspace/evaluate only and emits no settings or metadata. No native build or new semantic correctness claim; full CLIcheck follows the next frozen candidate.')
    save();print(json.dumps({'report':str(out/'report.json'),'passed':True,'actualNixExecution':False,'frontendTestsPassed':23}))

if __name__=='__main__':main()
