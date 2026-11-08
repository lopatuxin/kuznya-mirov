#!/bin/sh
# usage: run.sh <outdir-name> <shot names...>  — renders listed close-ups (all when none) into fix/<outdir>
cd /c/projects/kuznya-mirov
F=C:/projects/kuznya-mirov/art/transport/estate/garage/work/fix
mkdir -p $F/$1
[ -f $F/$1/shots.json ] || cp $F/after/shots.json $F/$1/shots.json
W=$F/$1/_only.py
n=$1; shift
if [ $# -gt 0 ]; then
  printf 'only = [%s]\n' "$(printf '"%s",' "$@")" > $W
else
  printf '' > $W
fi
printf 'exec(open(r"%s/after/check.py", encoding="utf-8").read())\n' "$F" >> $W
"C:/Program Files/Blender Foundation/Blender 5.2/blender.exe" --background --factory-startup $F/../calib.blend --python C:/Users/lopat/.claude/plugins/cache/anton-toolkit-marketplace/blender-modeling/0.2.3/scripts/run_parts.py -- art/transport/estate/garage $F/$n yard $W 2>&1 | grep "^RESULT" | head -c 600
