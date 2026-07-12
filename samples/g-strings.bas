10 CLG
20 SMALLFONT
30 MOVE WIDTH\2,HEIGHT\2
40 GDISP "hello","red","green"
50 GDISP "adios", "yellow", "blue"
60 MOVE WIDTH\2,YPOS-16
70 INK "cyan" : PAPER "green"
80 GDISP ":DDDDD"
90 BIGFONT OPAQUE
100 INK "red" : PAPER "green" : DISP "hello";
110 INK "yellow" : PAPER "blue" : DISP "adios";
120 DISP
130 INK "cyan" : PAPER "green"
140 DISP ":DDDDD"
150 INK 1 : PAPER 0

