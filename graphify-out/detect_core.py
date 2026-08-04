import json
from graphify.detect import detect
from pathlib import Path

# Core directories to analyze
core_dirs = ['crates', 'packages', 'ponytail', 'single-core-plaftform', 'docs']
all_files = {'code': [], 'document': [], 'paper': [], 'image': [], 'video': []}
total_words = 0

for core_dir in core_dirs:
    dir_path = Path(core_dir)
    if dir_path.exists():
        try:
            result = detect(dir_path)
            for ftype in all_files:
                all_files[ftype].extend(result.get('files', {}).get(ftype, []))
            total_words += result.get('total_words', 0)
            print(f'{core_dir}: {result.get("total_files", 0)} files')
        except Exception as e:
            print(f'Skipped {core_dir}: {e}')

merged = {
    'scan_root': str(Path('.').resolve()),
    'files': all_files,
    'total_files': sum(len(f) for f in all_files.values()),
    'total_words': total_words,
    'skipped_sensitive': []
}

Path('graphify-out/.graphify_detect.json').write_text(json.dumps(merged, ensure_ascii=False), encoding='utf-8')
print(f'Total: {merged["total_files"]} files, ~{total_words:,} words')
