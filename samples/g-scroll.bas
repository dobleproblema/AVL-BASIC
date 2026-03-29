10 LDIR 45 : BIGFONT : INK "yellow"
20 F=16 : V=2 : A=WIDTH\2 : B=0
30 T$="(c) 2025, Jose Antonio Avila - It really, *REALLY* works! · "
40 W=-360 'Point where the coordinates must be reset in this case
50 T$=T$+T$
60 MOVE A,B : GDISP T$,1,-1
70 A=A-V : B=B-V : IF A<=W THEN A=WIDTH\2:B=0
80 FRAME : CLG
90 GOTO 60

