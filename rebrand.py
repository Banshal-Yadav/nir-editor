import os
import re

ROOT_DIR = "crates"
ASSETS_DIR = "assets"
DOCS_DIR = "docs"

# 3.1 - "Zed" UI strings
def process_file_ui_strings(content, path):
    if path.endswith("zed.metainfo.xml.in") or path.endswith("zed.iss"):
        content = content.replace("Zed Editor", "/nir Editor")
        content = content.replace("Zed Industries", "/nir Industries")
        content = content.replace("Zed", "/nir")
    elif path.endswith("menus.rs") or "menu" in path:
        # Menus replacement
        content = content.replace('"Zed Docs"', '"/nir Docs"')
        content = content.replace('"Zed Community"', '"/nir Community"')
    return content

# 3.2 - zed.dev domain
def process_file_domain(content, path):
    # Skip test files and changelog
    if "test" in path or "CHANGELOG" in path or "changelog" in path.lower():
        return content

    # Telemetry and Collab
    content = re.sub(r'https://telemetry\.zed\.dev[^"\']*', '', content)
    content = re.sub(r'https://collab\.zed\.dev[^"\']*', '', content)
    
    # Docs
    content = re.sub(r'https://zed\.dev/docs[^"\']*', 'https://github.com/Banshal-Yadav/nir/wiki', content)
    
    # Community / Feedback
    content = re.sub(r'https://zed\.dev/community-links[^"\']*', 'https://github.com/Banshal-Yadav/nir/issues', content)
    content = re.sub(r'https://zed\.dev/faq[^"\']*', 'https://github.com/Banshal-Yadav/nir/wiki', content)
    
    # General / GitHub fallback for other zed.dev
    content = re.sub(r'https://zed\.dev[^"\']*', 'https://github.com/Banshal-Yadav/nir', content)
    content = re.sub(r'https://www\.zed\.dev[^"\']*', 'https://github.com/Banshal-Yadav/nir', content)
    content = re.sub(r'https://status\.zed\.dev[^"\']*', 'https://github.com/Banshal-Yadav/nir', content)

    return content

def process_directory(directory):
    for root, dirs, files in os.walk(directory):
        for file in files:
            if file.endswith(('.rs', '.json', '.toml', '.md', '.hbs', '.iss', '.xml.in')):
                filepath = os.path.join(root, file)
                try:
                    with open(filepath, 'r', encoding='utf-8') as f:
                        content = f.read()
                    
                    original_content = content
                    content = process_file_ui_strings(content, filepath)
                    content = process_file_domain(content, filepath)
                    
                    if content != original_content:
                        with open(filepath, 'w', encoding='utf-8') as f:
                            f.write(content)
                except Exception as e:
                    print(f"Failed to process {filepath}: {e}")

process_directory(ROOT_DIR)
process_directory(ASSETS_DIR)
process_directory(DOCS_DIR)

# Don't forget resources in crates/zed/resources
process_directory(os.path.join(ROOT_DIR, "zed", "resources"))
print("Done")
