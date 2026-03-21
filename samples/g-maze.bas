100 REM MAZE + SOLUTION (Random DFS/Prim Hybrid)
110 RANDOMIZE TIME
120 DFS=0.9 'DFS-style growth percentage vs Prim
130 Sol=-1   '-1 -> Draw the solution
140 OffX=20 'Minimum border
150 Cols=25 'Maze size. Between 3 and 100
160 Rows=Cols*3\4
170 Cell=(WIDTH-OffX*2)\Cols
180 OffX=(WIDTH-Cell*Cols)\2
190 OffY=(HEIGHT-Cell*Rows)\2
200 DIM Vis(Cols+1,Rows+1)
210 DIM Wall(Cols+1,Rows+1)
220 DIM ListX(Cols*Rows),ListY(Cols*Rows) 'Active list
230 DIM PX(Cols,Rows),PY(Cols,Rows)
240 DIM D(4)
250 REM **Initialize walls**
260 FOR Y=1 TO Rows
270   FOR X=1 TO Cols
280     Wall(X,Y)=15 : Vis(X,Y)=0
290   NEXT X
300 NEXT Y
310 Wall(1,1)=Wall(1,1)-1
320 Wall(Cols,Rows)=Wall(Cols,Rows)-4
330 REM **Hybrid algorithm**
340 SP=1 : ListX(1)=ListY(1)=Vis(1,1)=1 : PX(1,1)=PY(1,1)=0
350 WHILE SP>0
360   IF RND<DFS THEN R=SP ELSE R=INT(RND*SP)+1 'Choose DFS or Prim
370   CX=ListX(R) : CY=ListY(R)
380   GOSUB 710 'Choose an unvisited neighbor
390   IF Dir=-1 THEN ListX(R)=ListX(SP):ListY(R)=ListY(SP):SP=SP-1:GOTO 350
400   NX=CX+DX : NY=CY+DY
410   Wall(CX,CY)=Wall(CX,CY)-XBit
420   Wall(NX,NY)=Wall(NX,NY)-OppX
430   Vis(NX,NY)=1
440   SP=SP+1 : ListX(SP)=NX : ListY(SP)=NY
450   PX(NX,NY)=CX : PY(NX,NY)=CY
460 WEND
470 REM **Drawing**
480 SCREEN : CLG : INK 1
490 FOR Y=1 TO Rows
500   FOR X=1 TO Cols
510     PXp=(X-1)*Cell+OffX : PYp=(Y-1)*Cell+OffY
520     W=Wall(X,Y)
530     P=W\1 : IF P-P\2*2 THEN MOVE PXp,PYp:DRAW PXp,PYp+Cell           'W
540     P=W\2 : IF P-P\2*2 THEN MOVE PXp,PYp+Cell:DRAW PXp+Cell,PYp+Cell 'S
550     P=W\4 : IF P-P\2*2 THEN MOVE PXp+Cell,PYp+Cell:DRAW PXp+Cell,PYp 'E
560     P=W\8 : IF P-P\2*2 THEN MOVE PXp+Cell,PYp:DRAW PXp,PYp           'N
570   NEXT X
580 NEXT Y
590 REM **Solution path**
600 IF NOT Sol THEN END
610 INK 2 : PENWIDTH 2 : MASK &X00111100
620 HalfX=Cell\2+OffX : HalfY=Cell\2+OffY
630 CX=Cols : CY=Rows
640 WHILE CX<>1 OR CY<>1
650   NX=PX(CX,CY) : NY=PY(CX,CY)
660   MOVE (CX-1)*Cell+HalfX,(CY-1)*Cell+HalfY
670   DRAW (NX-1)*Cell+HalfX,(NY-1)*Cell+HalfY
680   CX=NX : CY=NY
690 WEND
700 END
710 REM **Random unvisited neighbor — returns Dir, DX, DY, XBit, OppX**
720 N=0
730 IF CY<Rows AND Vis(CX,CY+1)=0 THEN N=N+1:D(N)=1
740 IF CX<Cols AND Vis(CX+1,CY)=0 THEN N=N+1:D(N)=2
750 IF CY>1    AND Vis(CX,CY-1)=0 THEN N=N+1:D(N)=3
760 IF CX>1    AND Vis(CX-1,CY)=0 THEN N=N+1:D(N)=4
770 IF N=0 THEN Dir=-1:RETURN
780 Rn=INT(RND*N)+1 : Dir=D(Rn)
790 ON Dir GOTO 800, 810, 820, 830
800 DX=0 : DY=1 : XBit=2 : OppX=8 : RETURN
810 DX=1 : DY=0 : XBit=4 : OppX=1 : RETURN
820 DX=0 : DY=-1 : XBit=8 : OppX=2 : RETURN
830 DX=-1 : DY=0 : XBit=1 : OppX=4 : RETURN

