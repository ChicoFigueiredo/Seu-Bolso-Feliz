"""Confere o contrato de contraste dos 7 temas (WCAG 2): texto 4,5:1, contornos e foco 3:1.

Uso, a partir da raiz: python3 -I scripts/desktop/gerar-temas.py && python3 -I scripts/desktop/contraste.py
Saída com código 1 se algum par reprovar.
"""
import json, sys
from pathlib import Path
D = str(Path(__file__).resolve().parents[2] / "docs/design/desktop") + "/"
tok = json.load(open(D + "tokens.json", encoding="utf-8")); th = json.load(open(D + "temas.json", encoding="utf-8"))
base = {b: {t["name"]: t["value"][b] for t in tok["color"]["tokens"]} for b in ("dark", "light")}
def lin(c):
    c /= 255; return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4
def lum(h): return 0.2126*lin(int(h[1:3],16)) + 0.7152*lin(int(h[3:5],16)) + 0.0722*lin(int(h[5:7],16))
def cr(a, b):
    la, lb = sorted((lum(a[:7]), lum(b[:7])), reverse=True); return (la+.05)/(lb+.05)
S03 = ["surface-0","surface-1","surface-2","surface-3"]; S04 = S03 + ["surface-4"]; S02 = S03[:3]; S13 = S03[1:]
CHECKS = [  # (fg, [bg], minimo)
 ("ink", S04 + ["accent-subtle"], 4.5), ("ink-muted", S03 + ["accent-subtle"], 4.5),
 ("accent-ink", S13 + ["accent-subtle"], 4.5),
 ("on-accent", ["accent","accent-hover","accent-pressed"], 4.5),
 ("info", S13, 4.5), ("success", S13, 4.5), ("warning", S13, 4.5), ("danger", S13 + ["surface-4"], 4.5),
 ("info", ["info-subtle"], 4.5), ("success", ["success-subtle"], 4.5), ("warning", ["warning-subtle"], 4.5),
 ("danger", ["danger-subtle"], 4.5), ("gold-ink", ["gold-subtle"], 4.5),
 ("hero-text", ["hero-bg"], 4.5), ("hero-sub", ["hero-bg"], 4.5), ("hero-accent", ["hero-bg"], 4.5),
 ("toast-text", ["toast-bg"], 4.5),
 ("border-strong", S02, 3.0), ("focus-ring", S04 + ["accent-subtle"], 3.0),
 ("accent", S02, 3.0), ("toast-icon", ["toast-bg"], 3.0),
]
falhas = []; total = 0; linhas = []
for t in th["themes"]:
    c = dict(base[t["base"]]); c.update(t["colors"])
    for fg, bgs, mn in CHECKS:
        for bg in bgs:
            r = cr(c[fg], c[bg]); total += 1
            if r < mn: falhas.append((t["id"], fg, bg, c[fg], c[bg], r, mn))
print("pares conferidos:", total, "| reprovados:", len(falhas))
for f in falhas: print("FALHA %-8s %-14s sobre %-14s %s/%s = %.3f:1 (min %.1f)" % f)
# so informativo: borda decorativa
for t in th["themes"]:
    c = dict(base[t["base"]]); c.update(t["colors"])
    print(t["id"], "border/surface-1 = %.2f" % cr(c["border"], c["surface-1"]), "(decorativo, isento)")
sys.exit(1 if falhas else 0)
