import json
import os

template_path = r"c:\Users\bansa\OneDrive\Desktop\zed\zed\assets\themes\ayu\ayu.json"
output_dir = r"c:\Users\bansa\OneDrive\Desktop\zed\zed\assets\themes\void"

with open(template_path, 'r') as f:
    template = json.load(f)

# Use Ayu Dark as the base for dark themes
ayu_dark = template['themes'][0]
# Use Ayu Light as the base for light themes
ayu_light = template['themes'][1]

themes_data = [
    {
        "name": "Void Ember",
        "file": "void_ember.json",
        "appearance": "dark",
        "colors": {
            "bg": "#110a0a", "panel": "#1c1010", "editor": "#201414", "border": "#2e1818", 
            "text": "#f0d8d8", "muted": "#8a6060", "accent": "#e8a825"
        }
    },
    {
        "name": "Void Abyss",
        "file": "void_abyss.json",
        "appearance": "dark",
        "colors": {
            "bg": "#0d1212", "panel": "#131e1e", "editor": "#172020", "border": "#1e2e2e", 
            "text": "#d4f0f0", "muted": "#507070", "accent": "#e8a825"
        }
    },
    {
        "name": "Void Mono",
        "file": "void_mono.json",
        "appearance": "dark",
        "colors": {
            "bg": "#0f0f0f", "panel": "#141414", "editor": "#181818", "border": "#242424", 
            "text": "#e5e5e5", "muted": "#666666", "accent": "#e8a825"
        }
    },
    {
        "name": "Void Sand",
        "file": "void_sand.json",
        "appearance": "dark",
        "colors": {
            "bg": "#0f0f0d", "panel": "#1a1a16", "editor": "#1e1e1a", "border": "#2a2a22", 
            "text": "#f0f0d8", "muted": "#7a7a60", "accent": "#e8a825"
        }
    },
    {
        "name": "Void Gold",
        "file": "void_gold.json",
        "appearance": "dark",
        "colors": {
            "bg": "#0f0e0a", "panel": "#1a1810", "editor": "#1e1c12", "border": "#2a2818", 
            "text": "#fff0c0", "muted": "#806040", "accent": "#d4b860"
        }
    },
    {
        "name": "Void Nebula",
        "file": "void_nebula.json",
        "appearance": "dark",
        "colors": {
            "bg": "#0c0c10", "panel": "#141420", "editor": "#181828", "border": "#222230", 
            "text": "#e0d0ff", "muted": "#605080", "accent": "#e8a825"
        }
    },
    {
        "name": "Void Paper",
        "file": "void_paper.json",
        "appearance": "light",
        "colors": {
            "bg": "#f5f0e8", "panel": "#ede8dc", "editor": "#faf7f2", "border": "#d8d0c0", 
            "text": "#2a2418", "muted": "#8a7a60", "accent": "#b87800"
        },
        "syntax": {
            "keywords": "#b87800", "strings": "#0070a8", "types": "#007850", "comments": "#a09070"
        }
    },
    {
        "name": "Void Chalk",
        "file": "void_chalk.json",
        "appearance": "light",
        "colors": {
            "bg": "#f0f0f2", "panel": "#e8e8ec", "editor": "#f8f8fa", "border": "#d0d0d8", 
            "text": "#1a1a24", "muted": "#707080", "accent": "#6050d0"
        },
        "syntax": {
            "keywords": "#6050d0", "strings": "#0080c0", "types": "#008060", "comments": "#909098"
        }
    }
]

def apply_theme(base, data):
    new_style = base['style'].copy()
    c = data['colors']
    
    # Backgrounds
    new_style['background'] = c['bg']
    new_style['status_bar.background'] = c['bg']
    new_style['title_bar.background'] = c['bg']
    
    # Panels
    new_style['panel.background'] = c['panel']
    new_style['tab_bar.background'] = c['panel']
    new_style['tab.inactive_background'] = c['panel']
    new_style['surface.background'] = c['panel']
    new_style['elevated_surface.background'] = c['panel']
    new_style['element.background'] = c['panel']
    new_style['element.disabled'] = c['panel']
    new_style['ghost_element.disabled'] = c['panel']
    
    # Editor/Terminal
    new_style['editor.background'] = c['editor']
    new_style['editor.gutter.background'] = c['editor']
    new_style['terminal.background'] = c['editor']
    new_style['tab.active_background'] = c['editor']
    new_style['toolbar.background'] = c['editor']
    
    # Borders
    new_style['border'] = c['border']
    new_style['border.variant'] = c['border']
    new_style['border.disabled'] = c['border']
    new_style['scrollbar.track.border'] = c['border']
    new_style['editor.wrap_guide'] = c['border'] + "40" # Adding some transparency
    new_style['panel.focused_border'] = c['accent']
    
    # Text
    new_style['text'] = c['text']
    new_style['icon'] = c['text']
    new_style['editor.foreground'] = c['text']
    new_style['terminal.foreground'] = c['text']
    new_style['terminal.bright_foreground'] = c['text']
    
    # Muted
    new_style['text.muted'] = c['muted']
    new_style['icon.muted'] = c['muted']
    new_style['text.placeholder'] = c['muted']
    new_style['text.disabled'] = c['muted']
    new_style['icon.disabled'] = c['muted']
    new_style['icon.placeholder'] = c['muted']
    new_style['editor.line_number'] = c['muted']
    new_style['editor.invisible'] = c['muted'] + "40"
    
    # Accent
    new_style['text.accent'] = c['accent']
    new_style['icon.accent'] = c['accent']
    new_style['link_text.hover'] = c['accent']
    if 'players' in new_style:
        new_style['players'][0]['cursor'] = c['accent']
        new_style['players'][0]['background'] = c['accent']
        new_style['players'][0]['selection'] = c['accent'] + "3d"

    # Selection
    new_style['selection.background'] = c['accent'] + "25"
    
    # Syntax
    new_syntax = new_style['syntax'].copy()
    if data['appearance'] == 'dark':
        # Dark themes share syntax
        kw = "#e8a825"
        st = "#7dd3fc"
        ty = "#34d399"
        fn = c['muted'] # "muted based on theme text color"
        var = c['text'] # "primary text color"
        com = c['muted']
        num = "#e8a825"
        pun = c['muted']
    else:
        # Light themes have specific syntax
        s = data['syntax']
        kw = s['keywords']
        st = s['strings']
        ty = s['types']
        fn = c['muted']
        var = c['text']
        com = s['comments']
        num = kw # heuristic
        pun = c['muted']

    for key in ['keyword', 'preproc', 'boolean', 'constant', 'number', 'punctuation.special', 'selector', 'text.literal', 'string.special.symbol']:
        if key in new_syntax:
            new_syntax[key] = new_syntax[key].copy()
            new_syntax[key]['color'] = kw if key == 'keyword' else num
    
    new_syntax['keyword']['color'] = kw
    new_syntax['boolean']['color'] = num
    new_syntax['constant']['color'] = num
    new_syntax['number']['color'] = num
    
    new_syntax['string'] = new_syntax['string'].copy()
    new_syntax['string']['color'] = st
    if 'string.regex' in new_syntax:
        new_syntax['string.regex'] = new_syntax['string.regex'].copy()
        new_syntax['string.regex']['color'] = st
        
    new_syntax['type'] = new_syntax['type'].copy()
    new_syntax['type']['color'] = ty
    if 'enum' in new_syntax:
        new_syntax['enum'] = new_syntax['enum'].copy()
        new_syntax['enum']['color'] = ty
    if 'variant' in new_syntax:
        new_syntax['variant'] = new_syntax['variant'].copy()
        new_syntax['variant']['color'] = ty
        
    new_syntax['function'] = new_syntax['function'].copy()
    new_syntax['function']['color'] = fn
    
    new_syntax['variable'] = new_syntax['variable'].copy()
    new_syntax['variable']['color'] = var
    if 'property' in new_syntax:
        new_syntax['property'] = new_syntax['property'].copy()
        new_syntax['property']['color'] = var
    
    new_syntax['comment'] = new_syntax['comment'].copy()
    new_syntax['comment']['color'] = com
    new_syntax['comment']['font_style'] = "italic"
    
    new_syntax['punctuation'] = new_syntax['punctuation'].copy()
    new_syntax['punctuation']['color'] = pun
    
    new_style['syntax'] = new_syntax
    
    return {
        "name": data['name'],
        "appearance": data['appearance'],
        "style": new_style
    }

for theme_data in themes_data:
    base = ayu_light if theme_data['appearance'] == 'light' else ayu_dark
    theme_content = apply_theme(base, theme_data)
    
    final_json = {
        "$schema": "https://zed.dev/schema/themes/v0.2.0.json",
        "name": theme_data['name'],
        "author": "/void",
        "themes": [theme_content]
    }
    
    with open(os.path.join(output_dir, theme_data['file']), 'w') as f:
        json.dump(final_json, f, indent=2)

print("Done")
