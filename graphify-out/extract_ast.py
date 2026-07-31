import json
from pathlib import Path
from graphify.extract import extract_single_file

detect = json.loads(Path('graphify-out/.graphify_detect.json').read_text(encoding='utf-8'))
code_files = detect.get('files', {}).get('code', [])[:50]  # First 50 code files

nodes = []
edges = []
seen_ids = set()

print(f'Extracting AST from {len(code_files)} code files sequentially...')
for i, fpath in enumerate(code_files):
    if (i + 1) % 10 == 0:
        print(f'  {i+1}/{len(code_files)}...')
    try:
        result = extract_single_file(Path(fpath))
        for node in result.get('nodes', []):
            if node['id'] not in seen_ids:
                nodes.append(node)
                seen_ids.add(node['id'])
        edges.extend(result.get('edges', []))
    except:
        pass

result = {'nodes': nodes, 'edges': edges, 'input_tokens': 0, 'output_tokens': 0}
Path('graphify-out/.graphify_ast.json').write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding='utf-8')
print(f'AST extraction complete: {len(nodes)} nodes, {len(edges)} edges')
