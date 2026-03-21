100 REM 3D surface. Painter's algorithm with Lambert shading
105 DEG : T1=TIME
110 REM Resolution and graphics origin
115 SCRW=WIDTH : SCRH=HEIGHT
120 ORIGIN SCRW/2,SCRH/2 '(0,0) is now the center
125 REM Mesh and view
130 NX=50 : NY=50 : TOT=NX*NY*2
135 XR0=-2.5 : XR1=2.5 : YR0=-2.5 : YR1=2.5
140 DEF FNZ(X,Y)=(X^2+3*Y^2)*EXP(1-X^2-Y^2)
145 'DEF FNZ(X,Y)=2*X*EXP(-2*(X^2+Y^2))*4
150 'DEF FNZ(X,Y)=(X^3-Y^3)*EXP(-2*(X^2+Y^2))*10
155 'DEF FNZ(X,Y)=X^2*Y*EXP(-X^2-Y^2)*8
160 'DEF FNZ(X,Y)=(3*EXP(-(Y+1)^2-X^2)*(X-1)^2-EXP(-(X+1)^2-Y^2)/3+EXP(-X^2-Y^2)*(10*X^3-2*X+10*Y^5))/3
165 SCAL=80
170 ALPHA=235 : BETA=45
175 CA=COS(ALPHA) : SA=SIN(ALPHA) : CB=COS(BETA) : SB=SIN(BETA)
180 VXV=-SB*SA : VYV=-SB*CA : VZV=CB 'Vector toward the camera
185 AZ=135 : EL=25
190 LX=COS(EL)*COS(AZ) : LY=COS(EL)*SIN(AZ) : LZ=SIN(EL)
195 GRAD=0 : SHADE=1
200 LUZAMB=0.15 : LUZGAN=0.9
205 REM Arrays
210 DIM VX(NX+1,NY+1),VY(NX+1,NY+1),VZ(NX+1,NY+1)
215 DIM TD(TOT),TX0(TOT),TY0(TOT),TX1(TOT),TY1(TOT),TX2(TOT),TY2(TOT),TC(TOT)
220 DIM TIDX(TOT) 'Quicksort index array
225 DX=(XR1-XR0)/NX : DY=(YR1-YR0)/NY
230 ZMIN=1E+30 : ZMAX=-1E+30
235 REM Generate vertices and z-extremes
240 FOR I=0 TO NX
245   X=XR0+I*DX
250   FOR J=0 TO NY
255     Y=YR0+J*DY
260     Z=FNZ(X,Y)
265     VX(I,J)=X : VY(I,J)=Y : VZ(I,J)=Z
270     IF Z<ZMIN THEN ZMIN=Z
275     IF Z>ZMAX THEN ZMAX=Z
280   NEXT J
285 NEXT I
290 REM Build the triangles
295 K=-1
300 FOR I=0 TO NX-1
305   FOR J=0 TO NY-1
310     GOSUB 395 'Triangle 1: (i,j)-(i+1,j)-(i+1,j+1)
315     GOSUB 425 'Triangle 2: (i,j)-(i+1,j+1)-(i,j+1)
320   NEXT J
325 NEXT I
330 TOT=K+1 'Total after discarding triangles with back-face culling
335 GOSUB 580 'Sort indices by depth (Quicksort)
340 REM Draw using the order in the index array
345 CLG
350 FOR I=TOT-1 TO 0 STEP -1
355   K=TIDX(I) 'Get the index of the triangle to draw
360   INK TC(K)
365   FTRIANGLE TX0(K),TY0(K),TX1(K),TY1(K),TX2(K),TY2(K)
370   FRAME
375 NEXT I
380 PRINT "Elapsed time:";TIME-T1
385 END
390 REM Front triangle construction
395 X0=VX(I,J) : Y0=VY(I,J) : Z0=VZ(I,J)
400 X1=VX(I+1,J) : Y1=VY(I+1,J) : Z1=VZ(I+1,J)
405 X2=VX(I+1,J+1) : Y2=VY(I+1,J+1) : Z2=VZ(I+1,J+1)
410 GOSUB 450
415 RETURN
420 REM Rear triangle construction
425 X0=VX(I,J) : Y0=VY(I,J) : Z0=VZ(I,J)
430 X1=VX(I+1,J+1) : Y1=VY(I+1,J+1) : Z1=VZ(I+1,J+1)
435 X2=VX(I,J+1) : Y2=VY(I,J+1) : Z2=VZ(I,J+1)
440 GOSUB 450
445 RETURN
450 REM Main subroutine: back-face culling and triangle construction
455 REM Compute the normal and do back-face culling
460 UX=X1-X0 : UY=Y1-Y0 : UZ=Z1-Z0
465 VX=X2-X0 : VY=Y2-Y0 : VZ=Z2-Z0
470 NXV=UY*VZ-UZ*VY : NYV=UZ*VX-UX*VZ : NZV=UX*VY-UY*VX
475 IF NXV*VXV + NYV*VYV + NZV*VZV <= 0 THEN RETURN
480 K=K+1 : TIDX(K)=K 'Initialize index
485 REM Average triangle depth
490 D0=-SB*(X0*SA+Y0*CA)+CB*Z0
495 D1=-SB*(X1*SA+Y1*CA)+CB*Z1
500 D2=-SB*(X2*SA+Y2*CA)+CB*Z2
505 TD(K)=(D0+D1+D2)/3
510 REM Color based on height (Z)
515 ZAVG=(Z0+Z1+Z2)/3 : T=(ZAVG-ZMIN)/(ZMAX-ZMIN)
520 IF GRAD THEN GOSUB 680 ELSE GOSUB 700
525 IF SHADE THEN GOSUB 715
530 TC(K)=RGB(R,G,B)
535 REM Project the 3 vertices
540 X2P=X0*CA-Y0*SA : Y2P=X0*SA+Y0*CA : Y3P=Y2P*CB+Z0*SB
545 TX0(K)=X2P*SCAL : TY0(K)=Y3P*SCAL 'Project (X0,Y0,Z0)
550 X2P=X1*CA-Y1*SA : Y2P=X1*SA+Y1*CA : Y3P=Y2P*CB+Z1*SB
555 TX1(K)=X2P*SCAL : TY1(K)=Y3P*SCAL 'Project (X1,Y1,Z1)
560 X2P=X2*CA-Y2*SA : Y2P=X2*SA+Y2*CA : Y3P=Y2P*CB+Z2*SB
565 TX2(K)=X2P*SCAL : TY2(K)=Y3P*SCAL 'Project (X2,Y2,Z2)
570 RETURN
575 REM Algorithms
580 REM Quicksort (iterative, descending)
585 ST=0 : SL(ST)=0 : SR(ST)=TOT-1 'Initialize stack
590 WHILE ST>=0
595   L=SL(ST) : R=SR(ST) : ST=ST-1 'Pop
600   WHILE L<R
605     I=L : J=R
610     PVT=TD(TIDX(INT((L+R)/2))) 'Pivot through the index
615     WHILE TD(TIDX(I))>PVT : I=I+1 : WEND
620     WHILE TD(TIDX(J))<PVT : J=J-1 : WEND
625     IF I>J THEN GOTO 645
630     SWAP TIDX(I),TIDX(J) 'Only swap indices
635     I=I+1 : J=J-1
640     IF I<=J THEN GOTO 615
645     'Push the larger segment onto the stack, process the smaller one
650     IF (J-L)<(R-I) THEN SWAP L,I : SWAP J,R
655     IF L<J THEN ST=ST+1 : SL(ST)=L : SR(ST)=J
660     L=I
665   WEND
670 WEND 'While there are ranges left in the stack
675 RETURN
680 REM Rainbow color (phase-shifted sine waves)
685 H=T*360 : R=INT(127.5*(SIN(H)+1))
690 G=INT(127.5*(SIN(H+120)+1)) : B=INT(127.5*(SIN(H+120*2)+1))
695 RETURN
700 REM Linear blue-red color
705 R=INT(255*T) : G=0 : B=INT(255*(1-T))
710 RETURN
715 REM Flat shading (Lambert)
720 LNG=SQR(NXV*NXV+NYV*NYV+NZV*NZV)
725 IF LNG=0 THEN DOT=0:GOTO 750
730 DOT=(NXV*LX+NYV*LY+NZV*LZ)/LNG : IF DOT<0 THEN DOT=0
735 DOT=LUZAMB+LUZGAN*DOT
740 IF DOT>1 THEN DOT=1
745 R=INT(R*DOT) : G=INT(G*DOT) : B=INT(B*DOT)
750 RETURN

