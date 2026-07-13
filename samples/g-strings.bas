10 CLG
20 SMALLFONT
30 MOVE WIDTH\2,HEIGHT\2
40 LABEL "hello","red","green"
50 LABEL "adios", "yellow", "blue"
60 MOVE WIDTH\2,YPOS-16
70 INK "cyan" : PAPER "green"
80 LABEL ":DDDDD"
90 BIGFONT OPAQUE
100 INK "red" : PAPER "green" : GPRINT "hello";
110 INK "yellow" : PAPER "blue" : GPRINT "adios";
120 GPRINT
130 INK "cyan" : PAPER "green"
140 GPRINT ":DDDDD"
150 INK 1 : PAPER 0

