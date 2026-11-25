10 REM Horizontal text benchmark
20 SCREEN : MODE 640 : N=500000
30 PRINT "Horizontal text,";N;"draws per case (seconds)"
40 PRINT "Font";TAB(12);"Paper";TAB(27);"LABEL mixed";TAB(42);"LABEL inside";TAB(57);"GPRINT fixed"
50 FOR B=-1 TO 0
60   IF B=-1 THEN SMALLFONT TRANSPARENT ELSE SMALLFONT OPAQUE
70   CellW=8 : GOSUB Measure
80   IF B=-1 THEN BIGFONT TRANSPARENT ELSE BIGFONT OPAQUE
90   CellW=16 : GOSUB Measure
100 NEXT B
110 END

200 Measure:
210 REM Original case: part of the random range lies outside the screen.
220 CLG OFFSCREEN : RANDOMIZE 20261122 : T=TIME
230 FOR C=1 TO N
240   X=RND*(WIDTH+50)-50 : Y=RND*(HEIGHT+50)
250   MOVE X,Y
260   LABEL "I am here",INT(RND*32)+1,B
270 NEXT C
280 Mixed=TIME-T : FRAME

300 REM LABEL anchors the top left pixel. Both fonts are 16 pixels tall.
310 LabelW=CellW*LEN("I am here")
320 CLG OFFSCREEN : RANDOMIZE 20261122 : T=TIME
330 FOR C=1 TO N
340   X=RND*(WIDTH-LabelW) : Y=15+RND*(HEIGHT-16)
350   MOVE X,Y
360   LABEL "I am here",INT(RND*32)+1,B
370 NEXT C
380 Inside=TIME-T : FRAME

400 REM Redraw a fixed text cell over the scene, as in a game HUD.
410 T=TIME
420 FOR C=1 TO N
430   LOCATE 1,1 : GPRINT "I am here";
440 NEXT C
450 FixedCell=TIME-T : FRAME
460 PRINT IIF(CellW=8,"SMALLFONT","BIGFONT");TAB(12);IIF(B=-1,"TRANSPARENT","OPAQUE");
465 PRINT TAB(27);DEC$(Mixed,"0.000");TAB(42);DEC$(Inside,"0.000");TAB(57);DEC$(FixedCell,"0.000")
470 RETURN
