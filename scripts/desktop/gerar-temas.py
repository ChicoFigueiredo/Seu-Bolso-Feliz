"""Gera docs/design/desktop/tokens.json e temas.json a partir do tokens.css do SBF (valores verbatim).

Uso, a partir da raiz do repositório: python3 -I scripts/desktop/gerar-temas.py
"""
import json, re, sys
from pathlib import Path
RAIZ = Path(__file__).resolve().parents[2]
CSS = str(RAIZ / "packages/design-system/src/tokens.css")
OUT = str(RAIZ / "docs/design/desktop") + "/"
css = open(CSS, encoding="utf-8").read()
temas = {}
for m in re.finditer(r'\[data-sbf-theme="(\w+)"\]\s*\{(.*?)\n\}', css, re.S):
    temas[m.group(1)] = dict(re.findall(r'--([\w-]+):\s*([^;]+);', m.group(2)))
ORDEM = ["salvia", "ocre", "azul", "linho", "menta", "noite", "grafite"]
assert list(temas) == ORDEM, list(temas)
ESCUROS = {"noite", "grafite"}

def mix(a, b, t):  # a*(1-t)+b*t, hex #rrggbb
    ca = [int(a[i:i+2], 16) for i in (1, 3, 5)]; cb = [int(b[i:i+2], 16) for i in (1, 3, 5)]
    return "#" + "".join(f"{round(x*(1-t)+y*t):02x}" for x, y in zip(ca, cb))

def papeis(v, escuro):
    pressed = mix(v["primary"], v["bg"], 0.15) if escuro else mix(v["primary-dark"], v["text"], 0.30)
    return {
        "surface-0": v["bg"], "surface-1": v["surface"], "surface-2": v["surface-2"],
        "surface-3": v["hover-2"], "surface-4": v["line"],
        "border": v["line"], "border-strong": v["BORDA_FORTE"],
        "ink": v["text"], "ink-muted": v["text-3"], "ink-disabled": v["text-5"],
        "accent": v["primary"], "accent-hover": v["primary-dark"], "accent-pressed": pressed,
        "on-accent": v["on-primary"], "accent-subtle": v["primary-soft"],
        "accent-border": v["primary-line"], "accent-ink": v["primary-ink"],
        "focus-ring": v["text"],
        "info": v["secondary-ink"], "info-subtle": v["secondary-soft"], "info-border": v["secondary-line"],
        "success": v["success"], "success-subtle": v["primary-soft"],
        "warning": v["warn"], "warning-subtle": v["warn-soft"], "warning-border": v["warn-line"],
        "danger": v["danger"], "danger-subtle": v["warn-soft"],
        "gold": v["accent"], "gold-subtle": v["accent-soft"], "gold-ink": v["accent-ink"],
        "hero-bg": v["hero-bg"], "hero-text": v["hero-text"], "hero-sub": v["hero-sub"], "hero-accent": v["hero-accent"],
        "toast-bg": v["toast-bg"], "toast-text": v["toast-text"], "toast-icon": v["toast-icon"],
        "scrim": "#000000" + "8c" if escuro else v["text"] + "66",
    }

BORDA = "text-3"
cores = {}
for t in ORDEM:
    v = dict(temas[t]); v["BORDA_FORTE"] = v[BORDA]
    cores[t] = papeis(v, t in ESCUROS)

USO = {
 "surface-0": "Fundo da janela (SBF --bg).",
 "surface-1": "Fundo dos Painéis, do dock e de diálogos (SBF --surface).",
 "surface-2": "Ribbon, cabeçalhos de Painel, barra de status, campos de entrada (SBF --surface-2).",
 "surface-3": "Hover em itens, botões sutis e abas (SBF --hover-2).",
 "surface-4": "Pressionado; aba ativa do dock quando sem foco (SBF --line, derivado).",
 "border": "Divisórias e contorno de Painéis (decorativo, não carrega significado) (SBF --line).",
 "border-strong": "Contorno de controles (caixa de seleção, campo, Dropdown) e trilho do slider: 3:1 sobre surface-0, surface-1 e surface-2 (SBF --%s, derivado)." % BORDA,
 "ink": "Texto principal sobre surface-0 a surface-4 e sobre accent-subtle (SBF --text).",
 "ink-muted": "Texto secundário, rótulos de grupo do Ribbon, atalhos; 4,5:1 sobre surface-0 a surface-3 e sobre accent-subtle (SBF --text-3).",
 "ink-disabled": "Texto e ícones desabilitados (isento de contraste, nunca o único sinal) (SBF --text-5).",
 "accent": "Verde do SBF (--primary): botão primário, aba ativa do Ribbon, alternância ligada, slider, Painel ativo.",
 "accent-hover": "Hover do botão primário (SBF --primary-dark).",
 "accent-pressed": "Pressionado do botão primário (derivado: mistura de 30% (15% nos escuros) entre o par do SBF e o polo escuro do tema).",
 "on-accent": "Texto e ícone sobre accent, accent-hover e accent-pressed (SBF --on-primary).",
 "accent-subtle": "Fundo de item selecionado, de botão de alternância ligado e da aba ativa do Ribbon (SBF --primary-soft).",
 "accent-border": "Contorno de chip e selo no tom primário (SBF --primary-line). Decorativo.",
 "accent-ink": "Texto e ícone em accent sobre surface-1 a surface-3 e accent-subtle (link, valor ligado) (SBF --primary-ink).",
 "focus-ring": "Anel de foco do teclado, 2px sólido com 1px de surface por dentro; 3:1 em todas as superfícies (derivado: igual a ink).",
 "info": "Informação e IA; nunca identidade (SBF --secondary-ink, o tom de texto do azul ardósia). Texto com 4,5:1 sobre surface-1 a surface-3.",
 "info-subtle": "Fundo de chip e faixa informativa (SBF --secondary-soft).",
 "info-border": "Contorno de chip informativo (SBF --secondary-line). Decorativo.",
 "success": "Resultado positivo consolidado; sempre com ícone ou palavra. Texto com 4,5:1 sobre surface-1 a surface-3 (SBF --success).",
 "success-subtle": "Fundo de chip de sucesso (SBF --primary-soft, como no toneStyles).",
 "warning": "Compromisso próximo e atenção, em âmbar, nunca vermelho; sempre com ícone ou palavra (SBF --warn).",
 "warning-subtle": "Fundo de chip e faixa de atenção (SBF --warn-soft).",
 "warning-border": "Contorno de chip e faixa de atenção (SBF --warn-line). Decorativo.",
 "danger": "Risco real: atraso, falha, Ação destrutiva; sempre com ícone ou palavra (SBF --danger). Texto com 4,5:1 sobre surface-1 a surface-4.",
 "danger-subtle": "Fundo de chip de risco (SBF --warn-soft, como no toneStyles do SBF).",
 "gold": "Dourado fosco do SBF (--accent): conquista e meta, em doses pequenas. Marca, não texto.",
 "gold-subtle": "Fundo de selo de conquista (SBF --accent-soft).",
 "gold-ink": "Texto e ícone de conquista sobre gold-subtle (SBF --accent-ink).",
 "hero-bg": "Faixa de destaque com os dois números mais importantes da tela (SBF --hero-bg).",
 "hero-text": "Texto principal sobre hero-bg (SBF --hero-text).",
 "hero-sub": "Texto secundário sobre hero-bg (SBF --hero-sub).",
 "hero-accent": "Realce dourado sobre hero-bg (SBF --hero-accent).",
 "toast-bg": "Fundo do aviso transitório (SBF --toast-bg).",
 "toast-text": "Texto do aviso transitório (SBF --toast-text).",
 "toast-icon": "Ícone do aviso transitório (SBF --toast-icon).",
 "scrim": "Véu atrás de diálogos modais e da Paleta de Ações (derivado: preto 55% nos escuros; ink 40% nos claros).",
}
ordem_papeis = list(cores["salvia"].keys())
assert set(USO) == set(ordem_papeis)
tok = {
 "name": "Seu Bolso Feliz (desktop)", "version": 1,
 "color": {
  "themes": [{"id": "dark", "name": "Escuro"}, {"id": "light", "name": "Claro"}],
  "tokens": [{"name": n, "value": {"dark": cores["noite"][n], "light": cores["salvia"][n]}, "usage": USO[n]} for n in ordem_papeis],
 },
}
def est(name, size, lh, w, sample, usage, ls=None):
    d = {"name": name, "fontSize": f"{size}px", "lineHeight": f"{lh}px", "fontWeight": w, "sample": sample, "usage": usage}
    if ls: d["letterSpacing"] = ls
    return d
tok["type"] = {
 "fonts": [],
 "families": {
  "sans": "\"Manrope\", system-ui, -apple-system, \"Segoe UI\", Roboto, sans-serif",
  "serif": "\"Instrument Serif\", Georgia, \"Times New Roman\", serif",
  "icon": "\"Material Symbols Rounded\"",
  "mono": "\"JetBrains Mono\", ui-monospace, monospace",
 },
 "groups": [
  {"name": "Interface", "family": "sans", "styles": [
   est("caption", 12, 16, 400, "Ctrl+K · vence em 3 dias", "Atalhos, rótulos de grupo do Ribbon, botões pequenos do Ribbon, barra de status, metadados. É o menor tamanho do sistema."),
   est("ribbon-label", 12, 14, 400, "Nova movimentação", "Rótulo dos botões grandes do Ribbon clássico, em até duas linhas."),
   est("body", 14, 20, 400, "Fatura do cartão confirmada.", "Texto corrente de Painéis, itens de lista, controles."),
   est("body-strong", 14, 20, 600, "Nova movimentação", "Rótulos de botão, abas, cabeçalhos de Painel."),
   est("body-large", 16, 22, 400, "pagar fatura", "Campo de busca da Paleta de Ações."),
   est("subtitle", 16, 22, 600, "Compromissos do ciclo", "Títulos de seção em Opções e nos bastidores."),
   est("title", 20, 28, 600, "Opções", "Título de diálogo e de página dos bastidores.", "-0.01em"),
   est("display", 28, 36, 700, "Seu Bolso Feliz", "Só nos bastidores (Arquivo).", "-0.02em"),
  ]},
  {"name": "Números", "family": "sans", "styles": [
   est("amount", 14, 20, 600, "R$ 1.234,56", "Valor em dinheiro em listas, tabelas e cartões; sempre com números tabulares (tnum), para os dígitos não dançarem. Manrope tem tnum."),
   est("amount-large", 20, 28, 700, "R$ 12.480,00", "Total de um Painel ou de um grupo; números tabulares.", "-0.01em"),
  ]},
  {"name": "Destaque", "family": "serif", "styles": [
   est("hero-number", 40, 44, 400, "R$ 8.215", "Só os DOIS números mais importantes da tela (por exemplo, Disponível agora e Livre após compromissos), na faixa hero ou no topo do Painel. Instrument Serif só existe em peso 400; nunca em outro texto."),
  ]},
  {"name": "Técnico", "family": "mono", "styles": [
   est("log", 12, 18, 400, "job 4821 · hashed · ok", "Log e identificadores técnicos, em JetBrains Mono (OFL, a mesma do Chess Studio); nunca em texto de interface."),
  ]},
 ],
}
tok["spacing"] = {"note": "Grade de 4px, igual à do Chess. A Densidade Compacta usa um passo abaixo nos paddings. Os espaços 20 e 44 px do SBF web não entram.", "tokens": [
 {"name": n, "value": v, "usage": u} for n, v, u in [
  ("space-05", "2px", "Entre ícone e texto do selo; respiro interno mínimo."),
  ("space-1", "4px", "Entre controles de um grupo do Ribbon; padding vertical compacto."),
  ("space-2", "8px", "Padding de botão e campo; espaço entre itens de lista."),
  ("space-3", "12px", "Padding interno de Painel na densidade Compacta."),
  ("space-4", "16px", "Padding interno de Painel na Confortável; entre grupos de Opções."),
  ("space-6", "24px", "Margem de diálogo; entre seções dos bastidores."),
  ("space-8", "32px", "Margem externa dos bastidores."),
 ]]}
tok["radius"] = {"note": "Raios pequenos: interface de ferramenta, não de site. Os raios 8/12/16/22 px do SBF web não entram; pílula só em status e chips.", "tokens": [
 {"name": n, "value": v, "usage": u} for n, v, u in [
  ("radius-s", "2px", "Selos e marcadores."),
  ("radius-m", "4px", "Botões, campos, Dropdown, caixas de seleção, abas."),
  ("radius-l", "6px", "Painéis flutuantes, menus e tooltips."),
  ("radius-xl", "8px", "Diálogos, Paleta de Ações e bastidores."),
  ("radius-pill", "999px", "Só status e chips (e o Switch). Em mais nada."),
 ]]}
sc = lambda a: "rgba(24,35,31,%s)" % a
tok["shadow"] = {"tokens": [
 {"name": "shadow-flyout", "value": {"dark": "0 4px 12px rgba(0,0,0,0.45)", "light": "0 4px 12px " + sc(0.16)}, "usage": "Menus, Dropdown aberto, tooltips, KeyTips."},
 {"name": "shadow-floating", "value": {"dark": "0 8px 24px rgba(0,0,0,0.5)", "light": "0 8px 24px " + sc(0.2)}, "usage": "Painel flutuante."},
 {"name": "shadow-dialog", "value": {"dark": "0 16px 40px rgba(0,0,0,0.6)", "light": "0 16px 40px " + sc(0.24)}, "usage": "Diálogos modais e Paleta de Ações."},
]}
tok["size"] = {"note": "Alturas fixas na Escala 100%; a Escala da interface multiplica todas. Iguais às do Chess Studio.", "tokens": [
 {"name": n, "value": v, "usage": u} for n, v, u in [
  ("control-h", "32px", "Altura de botão, campo e Dropdown na densidade Confortável."),
  ("control-h-compact", "24px", "Altura de controle na densidade Compacta e alvo mínimo de clique: nada clicável é menor que 24px."),
  ("ribbon-tabs-h", "32px", "Faixa das abas do Ribbon."),
  ("ribbon-classic-h", "96px", "Corpo do Ribbon clássico: três linhas de 24px (alvo mínimo) + rótulo de grupo de 16px + 8px de respiro."),
  ("ribbon-simple-h", "40px", "Corpo do Ribbon simples, em uma linha."),
  ("panel-header-h", "32px", "Cabeçalho e abas de Painel."),
  ("statusbar-h", "24px", "Barra de status."),
  ("icon-s", "16px", "Ícone Material Symbols Rounded em botões, abas e Ribbon simples (peso 400, FILL 0, GRAD 0; opsz 20, o mínimo do eixo)."),
  ("icon-l", "32px", "Ícone grande do Ribbon clássico (opsz 32)."),
 ]]}
json.dump(tok, open(OUT + "tokens.json", "w", encoding="utf-8"), ensure_ascii=False, indent=1); open(OUT + "tokens.json", "a").write("\n")

INSP = {
 "salvia": ("Padrão", "O tema canônico: verde profundo sobre fundo quente claro."),
 "noite": ("Padrão", "O escuro canônico: verde-menta suave sobre grafite esverdeado."),
 "ocre": ("Claros", "Claro de pergaminho, com verde mais suave."),
 "azul": ("Claros", "Claro de fundo azulado frio."),
 "linho": ("Claros", "Claro de linho cru, neutro e quente."),
 "menta": ("Claros", "Claro de verde-menta fresco."),
 "grafite": ("Escuros", "Escuro neutro de grafite, com azul-ardósia."),
}
NOMES = {"salvia": "Sálvia", "ocre": "Ocre", "azul": "Azul", "linho": "Linho", "menta": "Menta", "noite": "Noite", "grafite": "Grafite"}
th = []
for t in ORDEM:
    esc = t in ESCUROS
    base = "noite" if esc else "salvia"
    diff = {k: x for k, x in cores[t].items() if t == base or True} if False else {k: x for k, x in cores[t].items() if cores[base][k] != x}
    if t in ("salvia", "noite"): diff = {}
    fam, ins = INSP[t]
    th.append({"id": t, "name": NOMES[t], "base": "dark" if esc else "light", "family": fam, "contrast": "aa", "inspiration": ins, "colors": diff})
json.dump({"version": 1,
 "note": "Catálogo dos Temas do Seu Bolso Feliz (desktop). Escuro (= noite) e Claro (= sálvia) ficam em tokens.json; cada Tema daqui parte de um deles (base) e troca só as cores listadas em colors. Salvia e noite têm colors vazio porque são as próprias bases. Os valores vêm verbatim de packages/design-system/src/tokens.css; as derivações estão no README.",
 "usage": "Aplique a base e depois as cores do Tema (o Visuals da base com colors por cima, em egui). Sombras vêm sempre da base.",
 "contract": "Todo Tema cumpre os mesmos pares de contraste: texto 4,5:1 e contornos, anel de foco e marcas 3:1 (WCAG 2 AA), exceto os pares reprovados listados no README.",
 "themes": th}, open(OUT + "temas.json", "w", encoding="utf-8"), ensure_ascii=False, indent=1)
open(OUT + "temas.json", "a").write("\n")
print("ok", BORDA)
