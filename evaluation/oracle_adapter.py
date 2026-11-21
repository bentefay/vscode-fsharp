#!/usr/bin/env python3
"""Expose the explicitly requested FCS helper through the evaluation protocol.

This is a correctness oracle experiment, not an Ionide/FSAC performance baseline.
The current helper deliberately invalidates compiler state before each query.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--dotnet', required=True)
    parser.add_argument('--helper', required=True)
    args = parser.parse_args()
    worker = None
    root, documents, count = None, {}, 0
    try:
        for line in sys.stdin:
            request = json.loads(line)
            method, params = request['method'], request.get('params', {})
            if method == 'workspace/open':
                root = params['root']
                worker = subprocess.Popen([str(Path(args.dotnet).resolve()), str(Path(args.helper).resolve())],
                                          stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=sys.stderr, text=True, bufsize=1)
                worker.stdin.write(json.dumps({'id': request['id'], 'method': 'capabilities', 'params': {}}) + '\n')
                worker.stdin.flush()
                response = json.loads(worker.stdout.readline())
                response['result']['baselineKind'] = 'FCS correctness oracle, not Ionide'
            elif method == 'workspace/stats':
                response = {'id': request['id'], 'result': {'compilerLaunches': int(worker is not None), 'oracleQueries': count}}
            elif method in ('document/open', 'document/change'):
                documents[params['path']] = params
                response = {'id': request['id'], 'result': {'version': params['version']}}
            elif method == 'document/close':
                documents.pop(params['path'], None)
                response = {'id': request['id'], 'result': {'closed': True}}
            else:
                if method.startswith('navigation/'):
                    params = {**params, 'root': root, 'documents': list(documents.values())}
                    count += 1
                worker.stdin.write(json.dumps({**request, 'params': params}) + '\n')
                worker.stdin.flush()
                response = json.loads(worker.stdout.readline())
            print(json.dumps(response), flush=True)
            if method == 'shutdown':
                break
    finally:
        if worker:
            try:
                worker.wait(timeout=3)
            except subprocess.TimeoutExpired:
                worker.kill(); worker.wait()


if __name__ == '__main__':
    main()
