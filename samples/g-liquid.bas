100 REM LIQUID - Water enters from below. Click after filling to make ripples.
110 SCREEN : MODE 960 : SMALLFONT TRANSPARENT : MAT BASE 0
120 N=16 : DX=12 : Side=N*DX : DT=1/120 : Gravity=730
130 Bed=71 : Rim=193 : Mark=126 : Center=WIDTH/2 : Iso=SQR(3)/2
140 Goal=Mark*Side*Side : FillSeconds=2.5 : InletX=10 : InletZ=12
150 DIM D(N+1,N+1),U(N,N),V(N,N),FX(N,N),FZ(N,N)
160 DIM PX(N+1,N+1),PY(N+1,N+1),Top(N+1,N+1)
170 DIM Weight(4) : FOR I=0 TO 4 : Weight(I)=(3-ABS(I-2))/9 : NEXT I
180 DEF FNX(X,Z)=Center+Iso*(X-Z)
190 DEF FNY(X,Z,Y)=Bed+(X+Z)/2+Y
200 REM X and Z lie on the floor. Y is height.
205 DEF FNCell(X)=1+MIN(N-1,MAX(0,INT(X/DX)))
210 Glass=RGB(137,186,202) : Shine=RGB(158,225,237)
220 LX=FNX(0,Side) : RX=FNX(Side,0) : EdgeY=FNY(0,Side,0) : Far=FNY(Side,Side,0)
230 FOR I=1 TO N+1 : FOR J=1 TO N+1
240   PX(I,J)=FNX((I-1)*DX,(J-1)*DX) : PY(I,J)=FNY((I-1)*DX,(J-1)*DX,0)
250 NEXT J : NEXT I
260 GOSUB PaintSet : GOSUB Reset
270 WHILE 1
280   K$=INKEY$
290   IF K$=CHR$(27) THEN SCREEN CLOSE : END
300   IF K$=" " THEN GOSUB Reset
302   IF Supplied>=Goal AND NOT Falling AND MOUSELEFT AND NOT WasDown THEN GOSUB AimDrop
304   WasDown=MOUSELEFT
310   FOR StepNo=1 TO 2 : GOSUB AdvanceWater : NEXT StepNo
320   SCREEN Back$
330   IF Supplied>0 THEN GOSUB DrawWater
335   IF Falling THEN FCIRCLE FNX(DropX,DropZ),FNY(DropX,DropZ,DropY),3.7,Shine
340   GOSUB FrontGlass
350   FRAME 60
360 WEND
370 Reset:
380 Supplied=0 : FillTime=0 : Falling=0 : WasDown=MOUSELEFT
390 MAT D=0 : MAT U=0 : MAT V=0 : MAT FX=0 : MAT FZ=0
400 RETURN

410 AdvanceWater:
420 REM Feed a small patch from below. The surface must move the water to the rest of the tank.
430 IF Supplied<Goal THEN
440   FillTime=MIN(FillSeconds,FillTime+DT) : Phase=FillTime/FillSeconds
450   NewTotal=Goal*Phase
460   Rise=(NewTotal-Supplied)/(DX*DX) : Supplied=NewTotal
470   FOR P=0 TO 4 : FOR Q=0 TO 4
480     ACol=InletX+P-2 : BCol=InletZ+Q-2
490     D(ACol,BCol)=D(ACol,BCol)+Rise*Weight(P)*Weight(Q)
500   NEXT Q : NEXT P
510 END IF
512 IF Falling THEN
514   DownSpeed=DownSpeed+Gravity*DT : DropY=DropY-DownSpeed*DT
516   IF DropY<=D(FNCell(DropX),FNCell(DropZ)) THEN Falling=0 : GOSUB Impact
518 END IF
520 REM A height difference drives flow across each shared face. The outer faces stay shut.
530 REM Speed is damped and capped so four outgoing flows cannot empty a column in one step.
540 FOR I=1 TO N-1 : FOR J=1 TO N
550   U(I,J)=MAX(-165,MIN(165,0.977*(U(I,J)+DT*535*(D(I,J)-D(I+1,J))/DX)))
560   V(J,I)=MAX(-165,MIN(165,0.977*(V(J,I)+DT*535*(D(J,I)-D(J,I+1))/DX)))
570   FX(I,J)=U(I,J)*IIF(U(I,J)>=0,D(I,J),D(I+1,J))*DT/DX
580   FZ(J,I)=V(J,I)*IIF(V(J,I)>=0,D(J,I),D(J,I+1))*DT/DX
590 NEXT J : NEXT I
600 REM Apply all transfers together. Each face adds exactly what it removes.
610 FOR I=1 TO N : FOR J=1 TO N
620   D(I,J)=D(I,J)+FX(I-1,J)-FX(I,J)+FZ(I,J-1)-FZ(I,J)
630 NEXT J : NEXT I
640 RETURN

650 DrawWater:
660 REM Average the four adjacent column heights at each mesh vertex.
670 FOR I=1 TO N+1 : FOR J=1 TO N+1
680   L=MAX(1,I-1) : R=MIN(N,I) : F=MAX(1,J-1) : B=MIN(N,J)
690   Top(I,J)=(D(L,F)+D(L,B)+D(R,F)+D(R,B))/4
700 NEXT J : NEXT I
710 REM Draw the two visible wet walls and then tile the surface from back to front.
720 FOR I=1 TO N : J=I+1
730   IF Top(I,1)+Top(J,1)>0.6 THEN TQUAD Water$,PX(I,1),PY(I,1),0,0,PX(J,1),PY(J,1),1,0,PX(J,1),PY(J,1)+Top(J,1),1,63.8,PX(I,1),PY(I,1)+Top(I,1),0,63.8
740   IF Top(1,I)+Top(1,J)>0.6 THEN TQUAD Water$,PX(1,I),PY(1,I),0,0,PX(1,J),PY(1,J),1,0,PX(1,J),PY(1,J)+Top(1,J),1,63.8,PX(1,I),PY(1,I)+Top(1,I),0,63.8
750 NEXT I
760 FOR I=N TO 1 STEP -1 : FOR J=N TO 1 STEP -1
770   H0=Top(I,J) : H1=Top(I+1,J) : H2=Top(I+1,J+1) : H3=Top(I,J+1)
780   IF H0+H1+H2+H3>1.2 THEN
790     Light=MAX(0,MIN(1,0.46+0.021*(H1+H2-H0-H3)+0.012*(H0+H1-H2-H3)))
800     Tint=RGB(19+64*Light,78+102*Light,110+99*Light)
810     X0=PX(I,J) : Y0=PY(I,J)+H0 : X1=PX(I+1,J) : Y1=PY(I+1,J)+H1
820     X2=PX(I+1,J+1) : Y2=PY(I+1,J+1)+H2 : X3=PX(I,J+1) : Y3=PY(I,J+1)+H3
830     FTRIANGLE X0,Y0,X1,Y1,X2,Y2,Tint : FTRIANGLE X0,Y0,X2,Y2,X3,Y3,Tint
840   END IF
850 NEXT J : NEXT I
860 FOR I=1 TO N : J=I+1
870   IF Top(I,1)+Top(J,1)>0.6 THEN DRAW PX(I,1),PY(I,1)+Top(I,1),PX(J,1),PY(J,1)+Top(J,1),Shine
880   IF Top(1,I)+Top(1,J)>0.6 THEN DRAW PX(1,I),PY(1,I)+Top(1,I),PX(1,J),PY(1,J)+Top(1,J),Glass
890 NEXT I
900 RETURN

910 PaintSet:
920 CLG OFFSCREEN
930 FOR Y=0 TO 63 : Light=(Y/63)^2
940   DRAW 0,Y,1,Y,RGB(8+23*Light,35+78*Light,55+96*Light)
950 NEXT Y : Water$=SPRITE$(0,0,1,63)
960 FOR Y=0 TO HEIGHT-1 : Light=MAX(0,1-ABS(Y-275)/310)
970   DRAW 0,Y,WIDTH-1,Y,RGB(10+12*Light,17+20*Light,28+23*Light)
980 NEXT Y
990 FCIRCLE Center,Bed+46,51,RGB(9,17,25),3.75
1000 Pane=RGB(34,61,76) : Edge=RGB(64,99,113) : FloorColor=RGB(36,57,69)
1010 FTRIANGLE LX,EdgeY,LX,EdgeY+Rim,Center,Far+Rim,Pane
1020 FTRIANGLE LX,EdgeY,Center,Far+Rim,Center,Far,Pane
1030 FTRIANGLE Center,Far,Center,Far+Rim,RX,EdgeY+Rim,RGB(39,67,80)
1040 FTRIANGLE Center,Far,RX,EdgeY+Rim,RX,EdgeY,RGB(39,67,80)
1050 DRAW LX,EdgeY+Rim,Center,Far+Rim,Edge : DRAW Center,Far+Rim,RX,EdgeY+Rim,Edge
1060 DRAW Center,Far,Center,Far+Rim,Edge
1070 FTRIANGLE LX,EdgeY,Center,Far,RX,EdgeY,FloorColor
1080 FTRIANGLE LX,EdgeY,RX,EdgeY,Center,Bed,FloorColor
1090 DRAW LX,EdgeY,Center,Bed,Edge : DRAW Center,Bed,RX,EdgeY,Edge
1100 MOVE WIDTH-409,25 : LABEL "CLICK  when full   SPACE  refill   ESC  exit",RGB(171,193,205)
1110 Back$=SCREEN$ : RETURN
1120 FrontGlass:
1130 FTRIANGLE LX,EdgeY,Center,Bed,Center,Bed+Rim,Pane ALPHA 0.12
1140 FTRIANGLE LX,EdgeY,Center,Bed+Rim,LX,EdgeY+Rim,Pane ALPHA 0.12
1150 DRAW LX,EdgeY,LX,EdgeY+Rim,Glass : DRAW RX,EdgeY,RX,EdgeY+Rim,Glass
1160 DRAW LX,EdgeY+Rim,Center,Bed+Rim,Glass : DRAW Center,Bed+Rim,RX,EdgeY+Rim,Glass
1170 DRAW Center,Bed,Center,Bed+Rim,Glass
1180 DRAW RX-26,EdgeY+Mark-15,RX-9,EdgeY+Mark-5,RGB(231,176,110)
1190 RETURN

1200 AimDrop:
1210 REM Invert the projection at the resting water level.
1220 Across=(MOUSEX-Center)/Iso : Along=2*(MOUSEY-Bed-Mark)
1230 PickX=(Along+Across)/2 : PickZ=(Along-Across)/2
1240 IF PickX<0 OR PickX>Side OR PickZ<0 OR PickZ>Side THEN RETURN
1250 DropX=PickX : DropZ=PickZ : DropY=Mark+92 : DownSpeed=0 : Falling=-1
1260 RETURN

1270 Impact:
1280 REM Push nearby face velocities outward. The wave moves water without adding any.
1290 FOR I=1 TO N-1 : FOR J=1 TO N
1300   FaceX=I*DX-DropX : FaceZ=(J-0.5)*DX-DropZ
1310   U(I,J)=U(I,J)+7*FaceX*MAX(0,1-(FaceX*FaceX+FaceZ*FaceZ)/900)
1320   FaceX=(J-0.5)*DX-DropX : FaceZ=I*DX-DropZ
1330   V(J,I)=V(J,I)+7*FaceZ*MAX(0,1-(FaceX*FaceX+FaceZ*FaceZ)/900)
1340 NEXT J : NEXT I
1350 RETURN
