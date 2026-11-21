#!/usr/bin/env python3
"""JSONL bridge to the real FSAC stdio server shipped by pinned Ionide.

Uses Ionide's AutomaticWorkspaceInit=false + fsharp/workspaceLoad sequence,
package defaults and server GC environment. It never calls InvalidateAll or the
experimental semantic helper. The measured tree includes this bridge and FSAC.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def package_config(package):
    config=json.loads(Path(package).read_text())['contributes']['configuration']
    if isinstance(config, list):
        props={key:value for group in config for key,value in group['properties'].items()}
    else:
        props=config['properties']
    settings={}
    for key,value in props.items():
        if not key.lower().startswith('fsharp.') or 'default' not in value:
            continue
        parts=key.split('.')[1:]
        target=settings
        for part in parts[:-1]:
            target=target.setdefault(part,{})
        target[parts[-1]]=value['default']
    return settings


def merge_config(target,overrides):
    for key,value in overrides.items():
        if isinstance(value,dict) and isinstance(target.get(key),dict):merge_config(target[key],value)
        else:target[key]=value


class Lsp:
    def __init__(self,command,env,cwd,config,timeout,trace):
        self.config=config
        self.timeout=timeout
        self.sequence=0
        self.pending={}
        self.lock=threading.Lock()
        self.events=[]
        self.condition=threading.Condition()
        self.stderr=[]
        self.trace=open(trace,'a') if trace else None
        self.process=subprocess.Popen(command,cwd=cwd,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        threading.Thread(target=self.read,daemon=True).start()
        threading.Thread(target=self.read_errors,daemon=True).start()

    def record(self,direction,message):
        if self.trace:
            self.trace.write(json.dumps({'time_ns':time.monotonic_ns(),'direction':direction,'message':message})+'\n')
            self.trace.flush()

    def write(self,message):
        data=json.dumps(message,separators=(',',':')).encode()
        with self.lock:
            self.record('client',message)
            self.process.stdin.write(f'Content-Length: {len(data)}\r\n\r\n'.encode()+data)
            self.process.stdin.flush()

    def config_section(self,section):
        value={'FSharp':self.config}
        for part in (section or 'FSharp').split('.'):
            value=value.get(part,{}) if isinstance(value,dict) else None
        return value

    def read(self):
        try:
            while True:
                headers={}
                while True:
                    line=self.process.stdout.readline()
                    if not line:raise EOFError('FSAC stdout closed')
                    if line in (b'\r\n',b'\n'):break
                    key,value=line.decode().split(':',1)
                    headers[key.lower()]=value.strip()
                message=json.loads(self.process.stdout.read(int(headers['content-length'])))
                self.record('server',message)
                if 'method' in message:
                    if 'id' in message:
                        method=message['method']
                        if method=='workspace/configuration':
                            result=[self.config_section(item.get('section')) for item in message.get('params',{}).get('items',[])]
                        elif method in ('client/registerCapability','client/unregisterCapability','window/workDoneProgress/create','workspace/semanticTokens/refresh','workspace/inlayHint/refresh','workspace/codeLens/refresh','workspace/diagnostic/refresh'):
                            result=None
                        elif method=='workspace/workspaceFolders':
                            result=self.workspace_folders
                        elif method=='window/showMessageRequest':
                            result=None
                        else:
                            self.write({'jsonrpc':'2.0','id':message['id'],'error':{'code':-32601,'message':'Unsupported benchmark client request '+method}})
                            continue
                        self.write({'jsonrpc':'2.0','id':message['id'],'result':result})
                    else:
                        with self.condition:
                            self.events.append(message)
                            self.condition.notify_all()
                else:
                    waiter=self.pending.get(message.get('id'))
                    if waiter:waiter.put(message)
        except Exception as error:
            for waiter in list(self.pending.values()):waiter.put({'error':{'message':str(error)}})

    def read_errors(self):
        for line in iter(self.process.stderr.readline,b''):
            text=line.decode(errors='replace').rstrip()
            self.stderr.append(text)
            if len(self.stderr)>100:self.stderr.pop(0)

    def request(self,method,params=None):
        self.sequence+=1
        request_id=self.sequence
        waiter=queue.Queue();self.pending[request_id]=waiter
        self.write({'jsonrpc':'2.0','id':request_id,'method':method,'params':params})
        try:
            response=waiter.get(timeout=self.timeout)
        except queue.Empty:
            raise TimeoutError(f'FSAC timed out on {method}; stderr={self.stderr[-8:]}')
        finally:
            self.pending.pop(request_id,None)
        if 'error' in response:raise RuntimeError(f'{method}: {response["error"]}; stderr={self.stderr[-8:]}')
        return response.get('result')

    def notify(self,method,params=None):
        self.write({'jsonrpc':'2.0','method':method,'params':params})

    def wait_analyzed(self,uri,version,since):
        deadline=time.monotonic()+self.timeout
        with self.condition:
            while True:
                for message in self.events[since:]:
                    document=message.get('params',{}).get('textDocument',{})
                    if message.get('method')=='fsharp/documentAnalyzed' and document.get('uri')==uri and document.get('version')==version:
                        return message
                remaining=deadline-time.monotonic()
                if remaining<=0:raise TimeoutError(f'No documentAnalyzed for {uri} version {version}')
                self.condition.wait(remaining)

    def close(self):
        if self.process.poll() is None:
            try:self.request('shutdown');self.notify('exit')
            except Exception:pass
            try:self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:self.process.kill();self.process.wait()
        if self.trace:self.trace.close()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ionide',required=True,type=Path,help='Extracted pinned Ionide extension directory')
    parser.add_argument('--dotnet',required=True,type=Path)
    parser.add_argument('--timeout',type=float,default=120)
    parser.add_argument('--config',type=Path,help='Explicit override JSON; omitted means shipped Ionide defaults')
    parser.add_argument('--trace',type=Path,help='Raw LSP trace for protocol/readiness audit')
    parser.add_argument('--document-readiness',choices=['analyzed','none'],default='analyzed',help='Wait for the server versioned completion notification before querying changed buffers')
    args=parser.parse_args()
    extension=args.ionide.resolve();dotnet=args.dotnet.resolve()
    server=extension/'bin/net10.0/fsautocomplete.dll'
    settings=package_config(extension/'package.json')
    if args.config:merge_config(settings,json.loads(args.config.read_text()))
    gc=settings.get('fsac',{}).get('gc',{})
    env=dict(os.environ,DOTNET_ROOT=str(dotnet.parent),PATH=str(dotnet.parent)+os.pathsep+os.environ.get('PATH',''),
             DOTNET_GCDynamicAdaptationMode=str(int(gc.get('useDatas',True))),DOTNET_GCConserveMemory=str(gc.get('conserveMemory') or 0),
             DOTNET_GCServer=str(int(gc.get('server',True))),DOTNET_CLI_TELEMETRY_OPTOUT='1')
    lsp=None;documents={};pending={};provenance={};query_count=0
    try:
        for line in sys.stdin:
            request=json.loads(line);method=request['method'];params=request.get('params',{});result={}
            try:
                if method=='workspace/open':
                    root=Path(params['root']).resolve();cache=Path(params['cacheDir']).resolve();cache.mkdir(parents=True,exist_ok=True)
                    command=[str(dotnet),str(server),'--state-directory',str(cache)]
                    if settings.get('enableMSBuildProjectGraph'):command.append('--project-graph-enabled')
                    if settings.get('fcs',{}).get('transparentCompiler',{}).get('enabled'):command.append('--use-fcs-transparent-compiler')
                    lsp=Lsp(command,env,root,settings,args.timeout,args.trace)
                    folders=[{'uri':root.as_uri(),'name':root.name}];lsp.workspace_folders=folders
                    capabilities={'workspace':{'configuration':True,'workspaceFolders':True,'didChangeConfiguration':{'dynamicRegistration':True}},
                                  'textDocument':{'synchronization':{'dynamicRegistration':False,'didSave':True},'definition':{'linkSupport':True},
                                                  'references':{'dynamicRegistration':False},'hover':{'contentFormat':['markdown','plaintext']},
                                                  'publishDiagnostics':{'versionSupport':True}},'window':{'workDoneProgress':True}}
                    initialized=lsp.request('initialize',{'processId':os.getpid(),'rootPath':str(root),'rootUri':root.as_uri(),
                        'workspaceFolders':folders,'initializationOptions':{'AutomaticWorkspaceInit':False},'capabilities':capabilities,
                        'clientInfo':{'name':'FSharp Navigator matched FSAC benchmark','version':'1'}})
                    lsp.notify('initialized',{})
                    lsp.notify('workspace/didChangeConfiguration',{'settings':{'FSharp':settings}})
                    # Ionide sends absolute project paths in TextDocuments.Uri.
                    selected=params.get('projects')
                    if selected is not None:
                        if not isinstance(selected,list) or not selected:raise ValueError('projects must be a nonempty explicit project list')
                        projects=[str((Path(item) if Path(item).is_absolute() else root/str(item)).resolve()) for item in selected]
                        if any(not Path(item).is_file() or Path(item).suffix.lower()!='.fsproj' for item in projects):raise ValueError('Explicit selected project is absent or not an fsproj')
                    else:
                        projects=sorted(str(p) for p in root.rglob('*.fsproj') if not any(x in {'obj','bin','node_modules','.git'} for x in p.relative_to(root).parts))
                    loaded=lsp.request('fsharp/workspaceLoad',{'TextDocuments':[{'Uri':path} for path in projects]})
                    content=json.loads(loaded['content'])
                    if content.get('Kind')!='workspaceLoad' or content.get('Data',{}).get('Status')!='finished':
                        raise RuntimeError('FSAC did not confirm completed workspaceLoad: '+repr(loaded))
                    provenance={'ionideVersion':json.loads((extension/'package.json').read_text())['version'],'serverSha256':sha(server),
                                'bridgeSha256':sha(__file__),'configOverrideSha256':sha(args.config) if args.config else None,
                                'installedRuntimeVersions':sorted(p.name for p in (dotnet.parent/'shared/Microsoft.NETCore.App').iterdir() if p.is_dir()),
                                'runtimeconfigSha256':sha(server.with_name('fsautocomplete.runtimeconfig.json')),
                                'packageSha256':sha(extension/'package.json'),'dotnetSha256':sha(dotnet),'command':command,
                                'gcEnvironment':{k:v for k,v in env.items() if k.startswith('DOTNET_GC')},'settings':settings,
                                'projects':projects,'projectSelection':'explicit' if selected is not None else 'recursive-fsproj-discovery','initializeResult':initialized,'workspaceLoadResult':loaded,
                                'scope':'FSAC plus Python stdio bridge; excludes IDE and controller','documentReadiness':args.document_readiness,
                                'readinessPolicy':'queried-document-and-pending-changed-dependencies-v2'}
                    result={'baselineKind':'Ionide-shipped FSAC stdio, normal persistent compiler state','provenance':provenance,'compilerLaunches':1,'backendPid':lsp.process.pid}
                elif method=='workspace/stats':
                    result={'compilerLaunches':int(lsp is not None),'baselineQueries':query_count,'serverPid':lsp.process.pid if lsp else None}
                elif method in ('document/open','document/change'):
                    path=str(Path(params['path']).resolve());uri=Path(path).as_uri();version=params['version']
                    previous=pending.get(path)
                    pending[path]=(uri,version,len(lsp.events),method=='document/change' or bool(previous and previous[3]))
                    if path not in documents:
                        lsp.notify('textDocument/didOpen',{'textDocument':{'uri':uri,'languageId':'fsharp','version':version,'text':params['text']}})
                    else:
                        lsp.notify('textDocument/didChange',{'textDocument':{'uri':uri,'version':version},'contentChanges':[{'text':params['text']}]})
                    documents[path]=version;result={'version':version}
                elif method=='document/close':
                    path=str(Path(params['path']).resolve());lsp.notify('textDocument/didClose',{'textDocument':{'uri':Path(path).as_uri()}})
                    documents.pop(path,None);result={'closed':True}
                    pending.pop(path,None)
                elif method.startswith('navigation/'):
                    feature=method.split('/')[1];query_count+=1
                    readiness_started=time.perf_counter()
                    if args.document_readiness=='analyzed':
                        # Unrelated didOpen checks can be superseded/cancelled
                        # before their analysis notification. They are not a
                        # prerequisite for a cursor provider in another file.
                        # Changed dependencies still gate a reference-domain
                        # query; never turn an unready edit into a fast success.
                        queried_path=str(Path(params['path']).resolve())
                        ready_paths=[path for path,value in pending.items() if path==queried_path or value[3]]
                        for path in ready_paths:
                            uri,version,since,_=pending[path]
                            lsp.wait_analyzed(uri,version,since)
                            pending.pop(path,None)
                    readiness_ms=(time.perf_counter()-readiness_started)*1000
                    query={'textDocument':{'uri':Path(params['path']).resolve().as_uri()},'position':{'line':params['line'],'character':params['character']}}
                    if feature=='references':query['context']={'includeDeclaration':params.get('includeDeclaration',False)}
                    answer=lsp.request('textDocument/'+feature,query)
                    if feature=='hover':
                        contents=(answer or {}).get('contents')
                        def markdown(value):
                            if isinstance(value,str):return value
                            if isinstance(value,list):return '\n\n'.join(markdown(x) for x in value)
                            return value.get('value','') if value else ''
                        result={'hover':({'markdown':markdown(contents),'range':answer.get('range')} if answer else None),'completeness':'exact','source':'ionide-fsac'}
                    else:
                        locations=answer if isinstance(answer,list) else [answer] if answer else []
                        locations=[{'uri':x['targetUri'],'range':x.get('targetSelectionRange',x['targetRange'])} if 'targetUri' in x else x for x in locations]
                        result={'locations':locations,'completeness':'exact','source':'ionide-fsac'}
                    result['documentReadinessMs']=readiness_ms
                elif method=='shutdown':
                    lsp.close();lsp=None;result={'shutdown':True}
                else:
                    raise ValueError('Unsupported baseline adapter method '+method)
                response={'id':request['id'],'result':result}
            except Exception as error:
                response={'id':request.get('id'),'error':{'message':str(error),'serverStderr':lsp.stderr[-10:] if lsp else []}}
            print(json.dumps(response),flush=True)
            if method=='shutdown':break
    finally:
        if lsp:lsp.close()


if __name__=='__main__':
    main()
