#!/usr/bin/env python3
"""Compare executable commands/flags against all live-schema operations/fields."""
import json, subprocess, pathlib, sys
root=pathlib.Path(__file__).resolve().parent.parent
binary=sys.argv[1] if len(sys.argv)>1 else str(root/'target/debug/lightrag-cli')
s=json.loads((root/'spec/openapi.json').read_text())
rows=json.loads(subprocess.check_output([binary,'operations','--json']))
expected={(m.upper(),p) for p,item in s['paths'].items() for m in item if m in ('get','post','put','delete','patch','head','options')}
assert expected=={(r['method'],r['path']) for r in rows}
fields=0
for r in rows:
    help_text=subprocess.check_output([binary,r['command'],'--help'],text=True)
    names=[p['name'] for p in r.get('parameters') or []]
    names+=list((r.get('body_schema') or {}).get('properties',{}))
    for name in names:
        flag='--server-dry-run' if name=='dry_run' else '--'+name.replace('_','-')
        assert flag in help_text,(r['command'],flag)
        fields+=1
    assert '--body' in help_text
print(json.dumps({'operations':len(rows),'fields':fields,'missing_operations':0,'missing_flags':0,'api_version':s['info']['version']}))
