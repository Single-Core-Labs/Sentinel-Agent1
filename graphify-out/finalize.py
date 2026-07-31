import json
from pathlib import Path
from datetime import datetime, timezone

# Save manifest
detect = json.loads(Path('graphify-out/.graphify_detect.json').read_text(encoding='utf-8-sig'))
extract = json.loads(Path('graphify-out/.graphify_extract.json').read_text(encoding='utf-8'))

# Update cumulative cost tracker
input_tok = extract.get('input_tokens', 0)
output_tok = extract.get('output_tokens', 0)
total_files = detect.get('total_files', 0)

cost_path = Path('graphify-out/cost.json')
cost = {
    'runs': [{
        'date': datetime.now(timezone.utc).isoformat(),
        'input_tokens': input_tok,
        'output_tokens': output_tok,
        'files': total_files,
    }],
    'total_input_tokens': input_tok,
    'total_output_tokens': output_tok,
}
cost_path.write_text(json.dumps(cost, indent=2, ensure_ascii=False), encoding='utf-8')

print(f'Extraction: {input_tok:,} input tokens, {output_tok:,} output tokens')
print(f'Total files analyzed: {total_files}')
