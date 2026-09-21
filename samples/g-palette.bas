10 SCREEN : MODE 640 : SMALLFONT TRANSPARENT : DEG : LDIR -90
15 FOR Y=120 TO 360 STEP 120 : MOVE 0,Y : DRAW 640,Y : NEXT Y
20 FOR X=80 TO 560 STEP 80 : MOVE X,0 : DRAW X,480 : NEXT X
25 FOR C=0 TO 31
30   X=C MOD 8 : Y=C\8 : READ N$
35   MOVE 80*X+40,420-120*Y : FILL C
40   INK 0
45   IF C=0 OR C=3 OR C=4 OR C=9 OR C=10 OR C=19 OR C=27 OR C=28 OR C=30 THEN INK 1
50   LOCATE 4+10*X,3+8*Y-Y\2 : GPRINT USING "0#";C;
55   MOVE 80*X+18,475-120*Y : LABEL N$
60 NEXT C : LDIR 0
65 DATA black, white, red, green, blue, yellow, magenta, cyan, darkorange
70 DATA purple, brown, gray, lightgreen, lightblue, lightgray, mediumpurple
75 DATA lightcyan, hotpink, gold, indigo, violet, steelblue, salmon, khaki
80 DATA pink, olive, lime, navy, teal, tan, maroon, ivory
