import os
import re

crates_dir = os.path.join(os.getcwd(), 'crates')

pattern = re.compile(r'sentinel-([a-z0-9-]+)\s*=\s*\{\s*path\s*=\s*"[^"]+"\s*(,\s*optional\s*=\s*true)?\s*\}')

for root, dirs, files in os.walk(crates_dir):
    for file in files:
        if file == 'Cargo.toml':
            filepath = os.path.join(root, file)
            with open(filepath, 'r', encoding='utf-8') as f:
                content = f.read()

            def repl(m):
                crate_name = m.group(1)
                opt = m.group(2) or ''
                return f'sentinel-{crate_name} = {{ workspace = true{opt} }}'

            new_content = pattern.sub(repl, content)
            if new_content != content:
                with open(filepath, 'w', encoding='utf-8') as f:
                    f.write(new_content)
                print(f'Updated {filepath}')
