"""Translate native ANSI styles while preserving the exact original code."""
import html
import itertools
import re

SGR=re.compile(r'\x1b\[([0-9;]*)m')
STANDARD=['#000000','#800000','#008000','#808000','#000080','#800080','#008080','#c0c0c0',
          '#808080','#ff0000','#00ff00','#ffff00','#0000ff','#ff00ff','#00ffff','#ffffff']

def ansi_color(n):
    if n<16:return STANDARD[n]
    if n>=232:return '#'+f'{8+10*(n-232):02x}'*3
    values=[0,95,135,175,215,255];n-=16
    return '#'+''.join(f'{values[v]:02x}' for v in (n//36,(n//6)%6,n%6))

def styled_chars(value):
    style=('#ffffff',False,False);pos=0;out=[]
    for m in SGR.finditer(value):
        out.extend((char,style) for char in value[pos:m.start()])
        codes=[int(x or '0') for x in m[1].split(';')];i=0;color,bold,italic=style
        while i<len(codes):
            code=codes[i]
            if code==0:color,bold,italic='#ffffff',False,False
            elif code==1:bold=True
            elif code==3:italic=True
            elif code==22:bold=False
            elif code==23:italic=False
            elif 30<=code<=37:color=STANDARD[code-30]
            elif 90<=code<=97:color=STANDARD[code-90+8]
            elif code==38 and codes[i+1]==5:color=ansi_color(codes[i+2]);i+=2
            else:raise ValueError(('Unsupported ANSI style',codes))
            i+=1
        style=(color,bold,italic);pos=m.end()
    out.extend((char,style) for char in value[pos:])
    return out

def project_styles(source,ansi):
    # The console normalizes some spacing/capitalization even in raw rendering.
    # Apply its styles to the original characters: copying the manual stays exact.
    styled=[(char,style) for char,style in styled_chars(ansi) if not char.isspace()]
    original=[c for c in source if not c.isspace()]
    assert ''.join(c for c,_ in styled).casefold()==''.join(original).casefold(),('Highlighter changed content',source,SGR.sub('',ansi))
    iterator=iter(styled);out=[]
    for char in source:
        if char.isspace():out.append((char,None))
        else:out.append((char,next(iterator)[1]))
    html_parts=[]
    for style,group in itertools.groupby(out,key=lambda x:x[1]):
        text=''.join(c for c,_ in group)
        if style is None:html_parts.append(html.escape(text));continue
        color,bold,italic=style
        declarations=f'color:{color};font-weight:{700 if bold else 400};font-style:{"italic" if italic else "normal"}'
        html_parts.append(f'<span style="{declarations}">'+html.escape(text)+'</span>')
    return ''.join(html_parts)
