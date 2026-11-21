#!/usr/bin/env python3
"""Materialize preserved experiment sources; does not install or alter production."""
from pathlib import Path
import shutil
here=Path(__file__).resolve().parent;root=here.parents[2]
for mode in ('il','r2r','aot'):
 dest=root/'.cache/decompiler-patched'/mode;dest.mkdir(parents=True,exist_ok=True)
 shutil.copyfile(here/'patched/Program.cs.txt',dest/'Program.cs')
 shutil.copyfile(here/'patched'/mode/'FSharpNavigator.Decompiler.csproj.txt',dest/'FSharpNavigator.Decompiler.csproj')
 shutil.copyfile(here/'patched'/mode/'packages.lock.json',dest/'packages.lock.json')
 shutil.copyfile(root/'semantic/Decompiler/ILSpy-LICENSE.txt',dest/'ILSpy-LICENSE.txt')
 print(dest)
