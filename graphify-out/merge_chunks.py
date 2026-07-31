import json
import glob
from pathlib import Path

# Merge all chunk files into semantic extraction
chunks = sorted(glob.glob('graphify-out/.graphify_chunk_*.json'))
all_nodes = []
all_edges = []
all_hyperedges = []
total_in = 0
total_out = 0
seen_ids = set()

print(f'Merging {len(chunks)} chunk files...')
for i, chunk_path in enumerate(chunks):
    try:
        chunk_data = json.loads(Path(chunk_path).read_text(encoding='utf-8'))
        
        # Deduplicate nodes by ID
        for node in chunk_data.get('nodes', []):
            if node['id'] not in seen_ids:
                all_nodes.append(node)
                seen_ids.add(node['id'])
        
        all_edges.extend(chunk_data.get('edges', []))
        all_hyperedges.extend(chunk_data.get('hyperedges', []))
        total_in += chunk_data.get('input_tokens', 0)
        total_out += chunk_data.get('output_tokens', 0)
        print(f'  Chunk {i}: {len(chunk_data.get("nodes", []))} nodes, {len(chunk_data.get("edges", []))} edges')
    except Exception as e:
        print(f'  Chunk {i} error: {e}')

merged = {
    'nodes': all_nodes,
    'edges': all_edges,
    'hyperedges': all_hyperedges,
    'input_tokens': total_in,
    'output_tokens': total_out,
}

Path('graphify-out/.graphify_semantic.json').write_text(json.dumps(merged, indent=2, ensure_ascii=False), encoding='utf-8')
print(f'Semantic extraction complete: {len(all_nodes)} nodes, {len(all_edges)} edges, {total_in:,} tokens')
