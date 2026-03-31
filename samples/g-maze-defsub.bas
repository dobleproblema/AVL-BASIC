100 REM MAZE + SOLUTION (DEF SUB + LOCAL vs GOSUB)
110 REM Variant of g-maze.bas rewritten to compare DEF SUB locals with classic GOSUB state
120 RANDOMIZE TIME
130 DFS=0.9 'DFS-style growth percentage vs Prim
140 Sol=-1   '-1 -> Draw the solution
150 OffX=20 'Minimum border
160 Cols=25 'Maze size. Between 3 and 100
170 Rows=Cols*3\4
180 Cell=(WIDTH-OffX*2)\Cols
190 OffX=(WIDTH-Cell*Cols)\2
200 OffY=(HEIGHT-Cell*Rows)\2
210 DIM Vis(Cols+1,Rows+1)
220 DIM Wall(Cols+1,Rows+1)
230 DIM ListX(Cols*Rows),ListY(Cols*Rows) 'Active list
240 DIM PX(Cols,Rows),PY(Cols,Rows)
250 DIM DirDX(4),DirDY(4),DirBit(4),DirOpp(4),PickDir(4)
260 DirDX(1)=0 : DirDY(1)=1 : DirBit(1)=2 : DirOpp(1)=8
270 DirDX(2)=1 : DirDY(2)=0 : DirBit(2)=4 : DirOpp(2)=1
280 DirDX(3)=0 : DirDY(3)=-1 : DirBit(3)=8 : DirOpp(3)=2
290 DirDX(4)=-1 : DirDY(4)=0 : DirBit(4)=1 : DirOpp(4)=4
300 REM **DEF SUB routines**
310 DEF SUB OPENCELL(CX,CY,Dir)
320   LOCAL DX,DY,XBit,OppX,NX,NY
330   DX=DirDX(Dir) : DY=DirDY(Dir)
340   XBit=DirBit(Dir) : OppX=DirOpp(Dir)
350   NX=CX+DX : NY=CY+DY
360   Wall(CX,CY)=Wall(CX,CY)-XBit
370   Wall(NX,NY)=Wall(NX,NY)-OppX
380   Vis(NX,NY)=1
390   SP=SP+1 : ListX(SP)=NX : ListY(SP)=NY
400   PX(NX,NY)=CX : PY(NX,NY)=CY
410 SUBEND
420 DEF SUB DRAWCELL(X,Y)
430   LOCAL PXp,PYp,W,P
440   PXp=(X-1)*Cell+OffX : PYp=(Y-1)*Cell+OffY
450   W=Wall(X,Y)
460   P=W\1 : IF P-P\2*2 THEN MOVE PXp,PYp:DRAW PXp,PYp+Cell           'W
470   P=W\2 : IF P-P\2*2 THEN MOVE PXp,PYp+Cell:DRAW PXp+Cell,PYp+Cell 'S
480   P=W\4 : IF P-P\2*2 THEN MOVE PXp+Cell,PYp+Cell:DRAW PXp+Cell,PYp 'E
490   P=W\8 : IF P-P\2*2 THEN MOVE PXp+Cell,PYp:DRAW PXp,PYp           'N
500 SUBEND
510 DEF SUB DRAWSOL
520   LOCAL HalfX,HalfY,CX,CY,NX,NY
530   HalfX=Cell\2+OffX : HalfY=Cell\2+OffY
540   CX=Cols : CY=Rows
550   WHILE CX<>1 OR CY<>1
560     NX=PX(CX,CY) : NY=PY(CX,CY)
570     MOVE (CX-1)*Cell+HalfX,(CY-1)*Cell+HalfY
580     DRAW (NX-1)*Cell+HalfX,(NY-1)*Cell+HalfY
590     CX=NX : CY=NY
600   WEND
610 SUBEND
620 REM **Initialize walls**
630 FOR Y=1 TO Rows
640   FOR X=1 TO Cols
650     Wall(X,Y)=15 : Vis(X,Y)=0
660   NEXT X
670 NEXT Y
680 Wall(1,1)=Wall(1,1)-1
690 Wall(Cols,Rows)=Wall(Cols,Rows)-4
700 REM **Hybrid algorithm**
710 T0=TIME
720 SP=1 : ListX(1)=ListY(1)=Vis(1,1)=1 : PX(1,1)=PY(1,1)=0
730 WHILE SP>0
740   IF RND<DFS THEN R=SP ELSE R=INT(RND*SP)+1 'Choose DFS or Prim
750   CX=ListX(R) : CY=ListY(R)
760   GOSUB 1000
770   IF Dir=-1 THEN ListX(R)=ListX(SP):ListY(R)=ListY(SP):SP=SP-1:GOTO 730
780   CALL OPENCELL(CX,CY,Dir)
790 WEND
800 T1=TIME
810 REM **Drawing**
820 SCREEN : CLG : INK 1
830 FOR Y=1 TO Rows
840   FOR X=1 TO Cols
850     CALL DRAWCELL(X,Y)
860   NEXT X
870 NEXT Y
880 T2=TIME
890 REM **Solution path**
900 T3=T2
910 IF NOT Sol THEN 940
920   INK 2 : PENWIDTH 2 : MASK &X00111100
930   CALL DRAWSOL : T3=TIME
940 PRINT "Generation:";ROUND(T1-T0,2);"sec."
950 PRINT "Drawing:";ROUND(T2-T1,2);"sec."
960 IF Sol THEN PRINT "Solution:";ROUND(T3-T2,2);"sec."
970 PRINT "Total:";ROUND(T3-T0,2);"sec."
980 END
990 REM **GOSUB helper: choose an unvisited neighbor -> Dir**
1000 N=0
1010 IF CY<Rows AND Vis(CX,CY+1)=0 THEN N=N+1:PickDir(N)=1
1020 IF CX<Cols AND Vis(CX+1,CY)=0 THEN N=N+1:PickDir(N)=2
1030 IF CY>1    AND Vis(CX,CY-1)=0 THEN N=N+1:PickDir(N)=3
1040 IF CX>1    AND Vis(CX-1,CY)=0 THEN N=N+1:PickDir(N)=4
1050 IF N=0 THEN Dir=-1:RETURN
1060 RN=INT(RND*N)+1 : Dir=PickDir(RN)
1070 RETURN
