#!/usr/bin/env python3
"""Render NyashkOpto's faceplate (see hwrender.py). Run from the repo root."""
import os, sys
sys.path.insert(0, os.path.dirname(__file__))
from hwrender import *

ACC = (255, 166, 64)
p = Panel(800, 470, (0.14, 0.145, 0.155), ACC, (232, 230, 224, 235), (150, 150, 148, 225))
header(p, 'OPTO', 'OPTICAL LEVELING AMPLIFIER')
p.button('power', 728, 32, 64, 22, 'POWER')
labels = [str(v) for v in range(0, 101, 10)]
for id, cx, lbl in [('gain', 150, 'GAIN'), ('peak', 650, 'PEAK REDUCTION')]:
    p.knob(id, cx, 180, 104, 'alu', label=lbl, label_dy=112, lsz=9.5, lcol=p.ink,
           scale=dict(r=78, n=41, major=4, labels=labels, lsz=8), led=dict(r=67, n=31))
p.vu('vu', 280, 84, 240, 172, ACC, '')
p.readout('vu_label', 400, 238, size=8)
for id, x, t in [('vu_in', 348, 'IN'), ('vu_gr', 400, 'GR'), ('vu_out', 452, 'OUT')]:
    p.button(id, x, 282, 46, 20, t)
p.text('METER', 400, 302, 7, None, True, sp=1)
p.inset(30, 326, 770, 452)
p.text('MODE', 80, 342, 7, None, True, sp=1)
p.button('comp', 80, 366, 64, 22, 'COMP'); p.button('limit', 80, 396, 64, 22, 'LIMIT')
p.text('MAKEUP', 155, 342, 7, None, True, sp=1)
p.button('makeup', 155, 366, 64, 22, 'AUTO')
p.text('ANALOG', 230, 342, 7, None, True, sp=1)
p.button('an_off', 230, 366, 64, 22, 'OFF'); p.button('an_50', 213, 396, 31, 22, '50'); p.button('an_60', 247, 396, 31, 22, '60')
p.screen('wave', 290, 344, 250, 90)
p.text('WAVE  ·  GR', 415, 444, 6.5, None, True, sp=1)
for id, x, lbl in [('hf', 598, 'HF FOLLOW'), ('mix', 668, 'MIX'), ('trim', 738, 'TRIM')]:
    p.knob(id, x, 380, 44, 'alu', label=lbl, label_dy=46, scale=dict(r=27, n=11, tick=4), led=dict(r=22, n=15), bip=(id == 'trim'))
p.export(os.path.join(os.path.dirname(__file__), '..'), 'plugins/nyashkopto')
