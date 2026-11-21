"""Judge actual neutral-observer rows; never manufactures an engine witness.

UNRUN source proposal. The canonical scenario verdict, input admission and this
judge must all succeed on the same complete raw log. This is not a native oracle.
"""
import argparse
import ast
import json
import math
import re
from pathlib import Path


def require(value, message):
    if not value:
        raise ValueError(message)


def judge(text):
    lines = text.splitlines()
    def one(pattern):
        rows = [(i, m) for i, line in enumerate(lines) if (m := re.search(pattern, line))]
        require(len(rows) == 1, f'exactly one row required: {pattern}')
        return rows[0]
    require(not re.search(r'nif attach probe: (?:terminal|unavailable|recipient) ', text), 'observer reported unavailable/terminal/changed recipient')
    prepared, prepared_row = one(r'nif attach probe: preflight prepared shapes=2 triangles=(\d+) rig=0 bounds=')
    preflight = [(i, line) for i, line in enumerate(lines) if 'nif attach probe: preflight shape ' in line]
    require(len(preflight) == 2 and int(prepared_row[1]) > 0, 'two real drawable preflight fields required')
    prepared_fields = []
    for index, line in preflight:
        m = re.search(r'preflight shape index=(\d+) mesh=(.*?) material=(.*?) vertices=4 local_min=(\[[^\]]+\]) local_max=(\[[^\]]+\]) positions_finite=1 roughness=(\S+) roughness_bits=(\d+) roughness_finite=1', line)
        require(m is not None, 'invalid real ModelCtx preflight field')
        shape, mesh, material, low, high, roughness, bits = m.groups()
        low, high = ast.literal_eval(low), ast.literal_eval(high)
        require(len(low) == len(high) == 3 and all(math.isfinite(v) for v in [*low, *high, float(roughness)]), 'nonfinite preflight geometry/material')
        require(all(a <= b for a, b in zip(low, high)), 'invalid preflight bounds')
        prepared_fields.append({'line': index, 'index': int(shape), 'mesh': mesh, 'material': material, 'width': high[0] - low[0], 'roughness': float(roughness), 'roughnessBits': int(bits)})
    require({f['index'] for f in prepared_fields} == {0, 1} and len({f['mesh'] for f in prepared_fields}) == 2, 'preflight shape identities are not two distinct meshes')
    for field, width, gloss in zip(sorted(prepared_fields, key=lambda f: f['width']), (32., 64.), (11., 22.)):
        require(abs(field['width'] - width) <= 1e-5 and abs(field['roughness'] - math.sqrt(math.sqrt(2. / (gloss + 2.)))) <= 1e-6, 'preflight geometry/material pair mismatch')
    requested, _ = one(r'script: print "nvf attach requested [01]"')
    require(prepared < min(f['line'] for f in prepared_fields) <= max(f['line'] for f in prepared_fields) < requested, 'ModelCtx admission must precede Attach dispatch')
    marker, _ = one(r'script: print "nvf attach removal submitted [01]"')
    raw = [(i, line) for i, line in enumerate(lines) if 'nif attach probe: present ' in line]
    require(len(raw) == 2, 'exactly two physical field rows required')
    fields = []
    for index, line in raw:
        m = re.search(r'present time=(\S+) root=(\d+) entity=(\d+) mesh=(.*?) material=(.*?) vertices=(\d+) local_min=(\[[^\]]+\]) local_max=(\[[^\]]+\]) roughness=(\S+) roughness_bits=(\d+) ', line)
        require(m is not None, 'malformed physical field row')
        now, root, entity, mesh, material, vertices, low, high, roughness, bits = m.groups()
        low, high = ast.literal_eval(low), ast.literal_eval(high)
        require(len(low) == len(high) == 3, 'bounds must be three-component fields')
        values = [float(now), float(roughness), *low, *high]
        require(all(math.isfinite(v) for v in values), 'nonfinite physical field')
        require(int(vertices) == 4 and all(a <= b for a, b in zip(low, high)), 'authored vertex count and valid bounds required')
        fields.append({'line': index, 'time': float(now), 'root': root, 'entity': entity, 'mesh': mesh, 'material': material, 'width': high[0] - low[0], 'roughness': float(roughness), 'roughnessBits': int(bits)})
    require(len({f['root'] for f in fields}) == 1, 'different recipient roots')
    require(len({f['entity'] for f in fields}) == len({f['mesh'] for f in fields}) == 2, 'physical fields must have distinct entity/mesh identities')
    fields.sort(key=lambda f: f['width'])
    for f, width, gloss in zip(fields, (32., 64.), (11., 22.)):
        require(abs(f['width'] - width) <= 1e-5, 'authored geometry width mismatch')
        expected = math.sqrt(math.sqrt(2. / (gloss + 2.)))
        # Public gloss_roughness is f32; tolerate rounding without exchanging
        # unequal material payloads. These two expected values differ by >0.06.
        require(abs(f['roughness'] - expected) <= 1e-6, 'actual bound material does not match its geometry')
    captured, c = one(r'nif attach probe: captured time=(\S+) root=(\d+) count=2 distinct_entities=1 distinct_meshes=1')
    retained, r = one(r'nif attach probe: retained time=(\S+) root=(\d+) captured_count=2 matching_named=2 same_handles=1 same_ancestry=1')
    removed, d = one(r'nif attach probe: removed time=(\S+) root=(\d+) captured_entities=(\[[^\]]+\]) captured_count=2 gone=2 matching_named=0 host_live=1 selected_same=1')
    root = fields[0]['root']
    require(c[2] == r[2] == d[2] == root, 'capture/retention/removal root mismatch')
    require(set(ast.literal_eval(d[3])) == {int(f['entity']) for f in fields}, 'removal must name the exact captured entities')
    require(requested < min(f['line'] for f in fields) <= max(f['line'] for f in fields) < captured < retained < marker < removed, 'submission, physical capture, retention and removal order is invalid')
    require(max(f['time'] for f in fields) <= float(c[1]) <= float(r[1]) <= float(d[1]), 'world-clock physical observations moved backwards')
    return {'actualRawFieldJudgement': True, 'actualPreflightFields': prepared_fields, 'fields': fields, 'capturedRoot': root, 'removedCapturedEntityCount': 2, 'nativeOrPixelAcceptance': False}


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--log', type=Path, required=True)
    args = p.parse_args()
    print(json.dumps(judge(args.log.read_text(encoding='utf-8'))))


if __name__ == '__main__':
    main()
