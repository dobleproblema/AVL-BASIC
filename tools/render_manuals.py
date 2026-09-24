"""Build the offline Rust manuals with the interpreter's own syntax highlighting.

Run from any directory: python tools/render_manuals.py
Requires Python 3.10+ and Cargo. Writes only MANUAL.html and MANUAL.es.html
to dist by default. Optional audit files are written outside that directory.
Source fences declare runnable BASIC, immediate commands, or illustrative text.
Every nonblank source body line and copied code character is audited.
"""
from pathlib import Path
import argparse
import hashlib
import html
from html.parser import HTMLParser
import json
import os
import re
import subprocess
import tempfile

from manuals.highlight import project_styles

ROOT = Path(__file__).resolve().parents[1]
GITHUB_SOURCE = 'https://github.com/dobleproblema/AVL-BASIC/blob/main/'
ASSETS = Path(__file__).resolve().parent / 'manuals'
E = html.escape
HEADING = re.compile(r'^((?:[0-9]|1[0-2])\.\d*)\s+(.+)$')
BULLET = re.compile(r'^(\s*)[-*•+]\s+(.+)$')
NUMBERED = re.compile(r'^\d+\s+')
PALETTE = re.compile(r'^#\s+Colou?r\s+#', re.I)
TRACE = re.compile(r'^\[\d+\]')
COMMAND = re.compile(r'(?!)')
CSS = (ASSETS / 'style.css').read_text(encoding='utf-8')
JS = (ASSETS / 'manual.js').read_text(encoding='utf-8')

def inline(text):
    chunks = re.split(r'(`[^`]+`)', text)
    out = []
    for chunk in chunks:
        if chunk.startswith('`') and chunk.endswith('`'):
            value = chunk[1:-1]
            out.append('<code>'+E(value)+'</code>')
        else:
            value = E(chunk)
            value = re.sub(r'\*\*([^*]+)\*\*', r'<strong>\1</strong>', value)
            value = re.sub(r'https?://[^\s<]+', lambda m:'<a href="'+m[0]+'">'+m[0]+'</a>', value)
            value = re.sub(r'\b((?:section|sección|apartado)\s+)(\d+(?:\.\d+)?)(?!\d|\.\d)',
                           lambda m: '<a href="#s-'+m[2].replace('.', '-')+'">'+m[0]+'</a>', value, flags=re.I)
            out.append(value)
    return ''.join(out)

SYNTAX_CACHE = None

def error_lines(kind, text):
    selected={int(n) for n in kind.removeprefix('output error=').split(',')}
    if not selected or min(selected)<1 or max(selected)>len(text.split('\n')):
        raise ValueError('Output error line index outside the block: '+kind)
    return selected

def style_requests(text, kind):
    if kind in {'basic','console'}:return [('code',line) for line in text.split('\n')]
    if kind=='output trace':return [('trace',m[0]) for m in re.finditer(r'\[\d+\]',text)]
    if kind.startswith('output error='):
        selected=error_lines(kind,text)
        return [('error',line) for n,line in enumerate(text.split('\n'),1) if n in selected]
    return []

def syntax(text, runtime, kind):
    if kind.startswith('output error='):selected=error_lines(kind,text)
    if SYNTAX_CACHE is None:
        return E(text)
    if kind=='output trace':
        return ''.join(SYNTAX_CACHE[('trace',part)] if re.fullmatch(r'\[\d+\]',part) else E(part)
                       for part in re.split(r'(\[\d+\])',text))
    if kind.startswith('output error='):
        return '\n'.join(SYNTAX_CACHE[('error',line)] if n in selected else E(line)
                         for n,line in enumerate(text.split('\n'),1))
    return '\n'.join(SYNTAX_CACHE[('code',line)] for line in text.split('\n'))

class TextReader(HTMLParser):
    def __init__(self): super().__init__();self.parts=[]
    def handle_data(self,data): self.parts.append(data)
    def handle_starttag(self,tag,attrs):
        if tag in {'br','p','li','dt','dd','tr','td','th','h3'}:self.parts.append(' ')
    def handle_endtag(self,tag):
        if tag in {'p','li','dt','dd','tr','td','th','h3'}:self.parts.append(' ')

def normal(text): return ' '.join(text.split())
def plain_markup(text):
    text=re.sub(r'`([^`]+)`',r'\1',text)
    return re.sub(r'\*\*([^*]+)\*\*',r'\1',text)

class Renderer:
    def __init__(self,lang,runtime):self.lang=lang;self.runtime=runtime;self.items=[];self.codeblocks=[];self.code_kinds=[];self.counter=0;self.covered=[]
    def block(self,tag,body,raw,indices,cls=''):
        reader=TextReader();reader.feed(body)
        assert normal(''.join(reader.parts))==normal(raw), ('Content mismatch',indices,raw[:150],''.join(reader.parts)[:150])
        self.counter+=1;ident=f'b-{self.counter}'
        self.covered.extend(indices)
        self.items.append({'id':ident,'lines':[i+1 for i in indices],'text':raw})
        return f'<{tag} id="{ident}" data-source="{indices[0]+1}" class="{cls}">{body}</{tag}>'
    def code(self,lines,kind='text'):
        indices=[i for i,_ in lines];value='\n'.join(s for _,s in lines)
        self.codeblocks.append(value)
        self.code_kinds.append(kind)
        highlighted=E(value) if kind=='text' else syntax(value,self.runtime,kind)
        pre=self.block('pre','<code>'+highlighted+'</code>',value,[i for i,line in lines if line.strip()])
        copy='Copiar' if self.lang=='es' else 'Copy'
        labels={'basic':('AVL BASIC · Programa completo','AVL BASIC · Complete program'),
                'console':('AVL BASIC · Modo inmediato','AVL BASIC · Immediate mode'),
                'text':('Fragmento / salida','Fragment / output'),
                'output':('Salida de consola','Console output')}
        label=labels[kind.split()[0]][0 if self.lang=='es' else 1]
        return f'<div class="codebox" data-kind="{kind}"><div class="codebar"><span>{label}</span><button class="copy" type="button">{copy}</button></div>{pre}</div>'
    def render(self,lines):
        out=[];i=0
        while i<len(lines):
            n,line=lines[i];s=line.strip()
            if not s:i+=1;continue
            if s.startswith('```'):
                kind=s[3:]
                if kind not in {'basic','console','text','output trace'} and not re.fullmatch(r'output error=\d+(?:,\d+)*',kind):
                    raise ValueError(f'Unknown fence at line {n+1}: {s}')
                self.covered.append(n);rows=[];i+=1
                while i<len(lines) and lines[i][1].strip()!='```':
                    rows.append(lines[i]);i+=1
                if i==len(lines) or not rows:raise ValueError(f'Unclosed or empty fence at line {n+1}')
                self.covered.append(lines[i][0]);i+=1
                out.append(self.code(rows,kind));continue
            # Original palette: keep the exact left/right order and colour names.
            if PALETTE.match(s):
                rows=[(n,line)];i+=1
                while i<len(lines) and lines[i][1].strip():rows.append(lines[i]);i+=1
                cells=[]
                for j,(rn,row) in enumerate(rows):
                    values=row.split();assert len(values)==4,(rn,row)
                    tag='th' if j==0 else 'td'
                    cells.append('<tr>'+''.join(f'<{tag}>'+((f'<span class="swatch" style="background:{v}"></span>') if j and k in (1,3) else '')+E(v)+f'</{tag}>' for k,v in enumerate(values))+'</tr>')
                out.append(self.block('div','<table>'+''.join(cells)+'</table>',' '.join(v for _,v in rows),[rn for rn,_ in rows],'table-wrap'));continue
            # Error catalogue is a real table, never treated as BASIC code.
            if re.match(r'^1\s{1,}(?:Instrucci[oó]n|Instruction|Statement)',s):
                rows=[]
                while i<len(lines) and NUMBERED.match(lines[i][1].strip()):rows.append(lines[i]);i+=1
                content=''.join('<tr><td>'+E(row.strip().split(None,1)[0])+'</td><td>'+inline(row.strip().split(None,1)[1])+'</td></tr>' for _,row in rows)
                out.append(self.block('div','<table>'+content+'</table>',plain_markup(' '.join(row for _,row in rows)),[rn for rn,_ in rows],'table-wrap'));continue
            if NUMBERED.match(s):
                rows=[]
                while i<len(lines) and NUMBERED.match(lines[i][1].strip()):rows.append(lines[i]);i+=1
                # Range notation is a numbered prose list.
                if any('`' in row for _,row in rows):
                    body=''.join('<p>'+inline(row)+'</p>' for _,row in rows)
                    out.append(self.block('div',body,plain_markup(' '.join(row for _,row in rows)),[rn for rn,_ in rows]))
                else:raise ValueError(f'Unfenced numbered example at line {n+1}')
                continue
            if s.startswith('+ ') and line==line.lstrip():
                rows=[]
                while i<len(lines) and lines[i][1].startswith('+ '):rows.append(lines[i]);i+=1
                paired=[]
                for rn,row in rows:
                    value=row[2:].strip();parts=re.split(r' {2,}',value,maxsplit=1)
                    if len(parts)==2:paired.append((rn,parts))
                    else:paired.append((rn,[value,'']))
                if any(desc for _,(_,desc) in paired):
                    content=''.join('<dt>'+inline('`'+sig+'`' if '`' not in sig else sig)+'</dt><dd>'+inline(desc)+'</dd>' for _,(sig,desc) in paired)
                    raw=plain_markup(' '.join(sig+' '+desc for _,(sig,desc) in paired))
                    out.append(self.block('dl',content,raw,[rn for rn,_ in rows],'reference'))
                else:
                    for rn,(sig,_) in paired:out.append(self.block('div',inline('`'+sig+'`' if '`' not in sig else sig),plain_markup(sig),[rn],'syntax'))
                continue
            bullet=BULLET.match(line)
            if bullet:
                # Recursive indentation-based lists with wrapped prose retained.
                rendered,next_i=self.list_block(lines,i,len(bullet[1]));out.append(rendered);i=next_i;continue
            if COMMAND.match(s) and not s.startswith('`') and len(s)<180:
                rows=[lines[i]];i+=1
                while i<len(lines) and lines[i][1].strip() and COMMAND.match(lines[i][1].strip()):rows.append(lines[i]);i+=1
                out.append(self.code(rows));continue
            if TRACE.match(s):
                rows=[]
                while i<len(lines) and lines[i][1].strip():rows.append(lines[i]);i+=1
                out.append(self.code(rows));continue
            # A standalone short label preceding a list/example is a subheading.
            next_s=lines[i+1][1].strip() if i+1<len(lines) else ''
            is_label=((len(s)<80 or s.startswith(('Ejemplo','Example'))) and not s.endswith(('.', ';')) and not s.startswith(('`','"')) and not (next_s and not next_s.startswith('```') and not BULLET.match(next_s) and not NUMBERED.match(next_s) and not COMMAND.match(next_s) and not PALETTE.match(next_s) and not TRACE.match(next_s)))
            if is_label:
                out.append(self.block('h3',inline(s),plain_markup(s),[n]));i+=1;continue
            rows=[lines[i]];i+=1
            while i<len(lines) and lines[i][1].strip() and not lines[i][1].strip().startswith('```') and not BULLET.match(lines[i][1]) and not NUMBERED.match(lines[i][1].strip()) and not PALETTE.match(lines[i][1].strip()) and not TRACE.match(lines[i][1].strip()):
                rows.append(lines[i]);i+=1
            value=' '.join(row.strip() for _,row in rows)
            out.append(self.block('p',inline(value),plain_markup(value),[rn for rn,_ in rows]))
        return '\n'.join(out)
    def list_block(self,lines,i,indent):
        result=['<ul>']
        while i<len(lines):
            n,line=lines[i];m=BULLET.match(line)
            if not m or len(m[1])!=indent:break
            values=[m[2]];indices=[n];i+=1
            while i<len(lines) and lines[i][1].strip() and not BULLET.match(lines[i][1]) and lines[i][1].startswith(' '):
                indices.append(lines[i][0]);values.append(lines[i][1].strip());i+=1
            value=' '.join(values)
            result.append('<li>'+self.block('span',inline(value),plain_markup(value),indices))
            while i<len(lines):
                child=BULLET.match(lines[i][1])
                if child and len(child[1])>indent:
                    nested,i=self.list_block(lines,i,len(child[1]));result.append(nested)
                else:break
            result.append('</li>')
        result.append('</ul>');return ''.join(result),i

def page(title,lang,body):
    return f'<!doctype html>\n<html lang="{lang}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="color-scheme" content="light dark"><meta name="description" content="AVL BASIC — {E(title)}"><title>{E(title)} · AVL BASIC</title><style>{CSS}</style></head><body id="top">{body}<script>{JS}</script></body></html>'

def brand(lang):
    label='Ir al inicio del manual' if lang=='es' else 'Go to the start of the manual'
    return f'<a class="brand" href="#top" aria-label="{label}"><span class="brand-icon" aria-hidden="true">&gt;_</span>AVL BASIC</a>'

def manual_filename(lang):return 'MANUAL.es.html' if lang=='es' else 'MANUAL.html'

def build_manual(root,runtime,lang,output,version):
    filename='MANUAL.es.txt' if lang=='es' else 'MANUAL.txt'
    source=root/filename;data=source.read_bytes();lines=data.decode('utf-8-sig').splitlines()
    headings=[(i,m[1].rstrip('.'),m[2]) for i,s in enumerate(lines) if (m:=HEADING.match(s)) and not re.match(r'^-',m[2])]
    assert headings and headings[0][1]=='0'
    assert len(set(number for _,number,_ in headings))==len(headings)
    renderer=Renderer(lang,runtime);contents=[];toc=[];es=lang=='es'
    for ix,(start,number,title) in enumerate(headings):
        end=headings[ix+1][0] if ix+1<len(headings) else len(lines)
        sid='s-'+number.replace('.','-')
        body=renderer.render(list(enumerate(lines))[start+1:end])
        renderer.covered.append(start)
        toc.append(f'<li class="{"minor" if "." in number else "major"}"><a href="#{sid}"><span class="num">{number}</span><span>{E(plain_markup(title))}</span></a></li>')
        contents.append(f'<section class="section" id="{sid}" data-title="{E(number+" · "+plain_markup(title))}"><h2><span class="section-number">{number}</span><span class="section-title">{inline(title)}</span><a class="anchor" href="#{sid}" aria-label="{"Enlace a este apartado" if es else "Link to this section"}">#</a></h2>{body}</section>')
    expected=[i for i in range(headings[0][0],len(lines)) if lines[i].strip()]
    assert sorted(renderer.covered)==expected,('Missing/duplicate source lines',set(expected)-set(renderer.covered))
    title='Manual de referencia' if es else 'Reference manual'
    other='en' if es else 'es'
    source_url=GITHUB_SOURCE+filename
    toc_html=''.join(toc)
    body=f'''
<a class="skip" href="#content">{'Ir al contenido' if es else 'Skip to content'}</a>
<header class="topbar">{brand(lang)}<span class="badge">{runtime.upper()} {version}</span><nav class="toplinks" aria-label="{'Opciones' if es else 'Options'}"><a data-switch href="{manual_filename(other)}" lang="{other}">{'English' if es else 'Español'}</a><button id="theme" type="button">{'Oscuro' if es else 'Dark'}</button><button class="mobile-menu" id="menu" aria-expanded="false" aria-controls="sidebar" type="button">{'Índice' if es else 'Contents'}</button></nav></header>
<div class="layout"><aside class="sidebar" id="sidebar"><label class="search-label" for="search">{'Buscar en el manual' if es else 'Search this manual'}</label><div class="search-wrap"><input id="search" type="search" placeholder="{'Comando, función, concepto…' if es else 'Command, function, concept…'}" autocomplete="off" spellcheck="false"><button id="clear-search" type="button" aria-label="{'Borrar búsqueda' if es else 'Clear search'}">×</button></div><p class="search-note">{'Pulsa' if es else 'Press'} <kbd>/</kbd> {'para buscar · Esc para salir' if es else 'to search · Esc to clear'}</p><div id="results" hidden aria-live="polite"></div><nav id="toc-area" aria-label="{'Índice del manual' if es else 'Manual contents'}"><div class="nav-label">{'Contenido' if es else 'Contents'}</div><ol class="toc">{toc_html}</ol></nav><div class="sidebar-bottom">{'Disponible sin conexión' if es else 'Available offline'}<br><a href="{source_url}">{'TXT en GitHub' if es else 'TXT on GitHub'}</a> · <a href="{GITHUB_SOURCE}COPYING">MIT</a></div></aside>
<main id="content"><div class="hero"><div class="eyebrow">{'Documentación / ' if es else 'Documentation / '}{runtime.title()}</div><h1>{title}</h1><p class="intro">{'El lenguaje, sus comandos y todo lo que necesitas para crear con AVL BASIC.' if es else 'The language, its commands, and everything you need to create with AVL BASIC.'}</p><div class="meta"><span>{runtime.title()} {version}</span><span>{len(headings)} {'apartados' if es else 'sections'}</span><span>{len(renderer.codeblocks)} {'ejemplos y bloques de código' if es else 'examples and code blocks'}</span><span>HTML · 2026</span></div><div class="quicklinks"><a class="button primary" href="#s-1">{'Primeros pasos' if es else 'Quick start'} ↓</a><a class="button" href="#s-10">{'Funciones' if es else 'Functions'}</a><a class="button" href="#s-8-2">{'Códigos de error' if es else 'Error codes'}</a><button id="print" class="button" type="button">{'Imprimir / PDF' if es else 'Print / PDF'}</button></div></div><noscript><p class="noscript">{'Puedes leer el manual completo y usar el índice. Activa JavaScript para buscar y copiar ejemplos.' if es else 'The complete manual and contents work without JavaScript. Enable it to search and copy examples.'}</p></noscript>{''.join(contents)}<footer class="foot">{'Conversión HTML del manual original' if es else 'HTML conversion of the original manual'} · {runtime.title()} {version} · MIT<br><a href="{source_url}">{'Consultar TXT en GitHub' if es else 'Read TXT on GitHub'}</a></footer></main></div><a class="backtop" href="#top" aria-label="{'Volver al inicio' if es else 'Back to top'}">↑</a>'''
    target=output/manual_filename(lang);target.write_text(page(f'{title} · {runtime.title()} {version}',lang,body),encoding='utf-8')
    return {'file':target.name,'runtime':runtime,'version':version,'language':lang,'source':filename,'source_url':source_url,'sha256':hashlib.sha256(data).hexdigest(),'sections':len(headings),'code_blocks':len(renderer.codeblocks),'body_nonempty_lines':len(expected),'audited_render_blocks':len(renderer.items),'all_source_lines_rendered':True,'rendered_text_matches_source':True,'code':renderer.codeblocks,'code_kinds':renderer.code_kinds}

def native_styles(requests, helper=None):
    """Build against this checkout; never silently reuse a stale exported palette."""
    if helper is None:
        result = subprocess.run(
            ['cargo', 'build', '--locked', '--example', 'manual_highlight', '--message-format=json'],
            cwd=ROOT, capture_output=True, text=True, encoding='utf-8', check=True)
        artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
        helper = next(Path(a['executable']) for a in reversed(artifacts)
                      if a.get('reason') == 'compiler-artifact'
                      and a.get('target', {}).get('name') == 'manual_highlight' and a.get('executable'))
    styles={}
    for mode in sorted({mode for mode,_ in requests}):
        lines=[line for requested,line in requests if requested==mode]
        result = subprocess.run([str(helper),'--'+mode], input='\n'.join(lines)+'\n', capture_output=True,
                                text=True, encoding='utf-8', check=True,
                                env={**os.environ, 'AVL_BASIC_THEME':'dark'})
        highlighted = result.stdout.splitlines()
        if len(highlighted) != len(lines):
            raise ValueError('Native highlighter returned the wrong number of lines')
        styles.update({(mode,line):project_styles(line,ansi) for line,ansi in zip(lines,highlighted)})
    return styles


class Links(HTMLParser):
    def __init__(self):
        super().__init__(); self.ids=set(); self.links=[]
    def handle_starttag(self, tag, attrs):
        attrs=dict(attrs)
        if 'id' in attrs:
            if attrs['id'] in self.ids:raise ValueError('Duplicate id: '+attrs['id'])
            self.ids.add(attrs['id'])
        if tag=='a' and 'href' in attrs:self.links.append(attrs['href'])


def validate_links(output):
    from urllib.parse import unquote, urlsplit
    pages={path.name:Links() for path in output.glob('*.html')}
    for name, parser in pages.items():parser.feed((output/name).read_text(encoding='utf-8'))
    count=0
    for name, parser in pages.items():
        for href in parser.links:
            url=urlsplit(href)
            if url.scheme or url.netloc:continue
            target=unquote(url.path) or name
            if not (output/target).is_file():raise ValueError(f'{name}: missing link {href}')
            if url.fragment and (target not in pages or unquote(url.fragment) not in pages[target].ids):
                raise ValueError(f'{name}: missing anchor {href}')
            count+=1
    return count


def generate(output, helper=None, audit_dir=None):
    global SYNTAX_CACHE
    output.mkdir(parents=True, exist_ok=True)
    version=re.search(r'^version\s*=\s*"([^"]+)"', (ROOT/'Cargo.toml').read_text(encoding='utf-8'), re.M)[1]
    SYNTAX_CACHE=None
    # First render audits coverage and discovers every code line; the second adds
    # actual interpreter styles while leaving copying and whitespace unchanged.
    manifests=[build_manual(ROOT,'rust',lang,output,version) for lang in ('es','en')]
    unique=sorted({request for m in manifests for code,kind in zip(m['code'],m['code_kinds'])
                   for request in style_requests(code,kind)})
    SYNTAX_CACHE=native_styles(unique,helper)
    manifests=[build_manual(ROOT,'rust',lang,output,version) for lang in ('es','en')]
    audit={m['file']:[{'kind':kind,'code':code} for kind,code in zip(m['code_kinds'],m['code'])] for m in manifests}
    verified={'manuals':[{k:v for k,v in m.items() if k not in ('code','code_kinds')} for m in manifests],
              'local_links_verified':validate_links(output),
              'highlighter_source_sha256':hashlib.sha256((ROOT/'src/console.rs').read_bytes()).hexdigest(),
              'complete_programs':{m['language']:m['code_kinds'].count('basic') for m in manifests}}
    if audit_dir is not None:
        if audit_dir.resolve().is_relative_to(output.resolve()):
            raise ValueError('Audit files must be outside the HTML output directory')
        audit_dir.mkdir(parents=True,exist_ok=True)
        for name,content in [('code-audit.json',audit),('verification.json',verified)]:
            (audit_dir/name).write_text(json.dumps(content,ensure_ascii=False,indent=2)+'\n',encoding='utf-8',newline='\n')
    return verified


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,default=ROOT/'dist')
    mode=parser.add_mutually_exclusive_group()
    mode.add_argument('--check',action='store_true',help='Check generated files without changing the output directory')
    mode.add_argument('--audit-dir',type=Path,help='Optional developer reports, outside the HTML output directory')
    parser.add_argument('--highlighter',type=Path,help='Use an explicitly built manual_highlight executable')
    args=parser.parse_args()
    if args.audit_dir is not None and args.audit_dir.resolve().is_relative_to(args.output.resolve()):
        parser.error('--audit-dir must be outside --output')
    if args.check:
        with tempfile.TemporaryDirectory(prefix='avl-manual-check-') as tmp:
            generated=Path(tmp)
            verified=generate(generated,args.highlighter)
            changed=[str(p.relative_to(generated)) for p in generated.rglob('*') if p.is_file()
                     and (not (args.output/p.relative_to(generated)).is_file()
                          or p.read_bytes()!=(args.output/p.relative_to(generated)).read_bytes())]
            if changed:raise SystemExit('Stale or missing generated files: '+', '.join(changed))
    else:
        verified=generate(args.output,args.highlighter,args.audit_dir)
    print(json.dumps({'output':str(args.output), 'checked':args.check, **verified},ensure_ascii=False,indent=2))


if __name__=='__main__':main()
