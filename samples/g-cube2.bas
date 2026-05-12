100 REM ** Screen setup **
110 W=WIDTH : H=HEIGHT : SCREEN
120 Xc=W/2 : Yc=H/2
130 REM ** Angular speeds and orbit **
140 VrotY=24E-3 : VrotX=32E-3 'Cube rotation
150 DistCam = 8 'Distance to the camera
160 Rorb=3 'Orbit radius
170 Vorb=28E-3 'Orbital speed
180 RotY=0 : RotX=0 : OrbA=0
190 DIM P(3,8) 'Vertices
200 DIM V1x(8),V1y(8),CZ1(8) 'Cube 1: screen and Z
210 DIM V2x(8),V2y(8),CZ2(8) 'Cube 2: screen and Z
220 DIM F(4,6) 'Faces
230 REM ** Vertex data **
240 FOR I=1 TO 8 : READ P(1,I) : NEXT
250 FOR I=1 TO 8 : READ P(2,I) : NEXT
260 FOR I=1 TO 8 : READ P(3,I) : NEXT
270 REM ** Face data **
280 FOR I=1 TO 6 : FOR J=1 TO 4 : READ F(J,I) : NEXT J : NEXT I
290 T0=TIME : FPS=0
300 REM ** Main loop **
310   FRAME 60 : CLG
320   'Advance angles
330   RotY=RotY+VrotY
340   RotX=RotX+VrotX
350   OrbA=OrbA+Vorb
360   'Precompute sines and cosines
370   SY=SIN(RotY) : CY=COS(RotY)
380   SX=SIN(RotX) : CX=COS(RotX)
390   REM ** Orbital offsets for each cube (180° out of phase) **
400   OffX1=Rorb*COS(OrbA) : OffZ1=Rorb*SIN(OrbA)
410   OffX2=-OffX1 : OffZ2=-OffZ1 'Directly opposite
420   REM ** Compute the two projected-vertex lists **
430   Zsum1=0 : Zsum2=0
440   FOR I=1 TO 8
450     'Rotation
460     Yw=P(2,I)*CY-P(3,I)*SY
470     Zw=P(3,I)*CY+P(2,I)*SY
480     Xw=P(1,I)*CX-Zw*SX
490     Zw=Zw*CX+P(1,I)*SX+DistCam 'Push the whole system away
500     'CUBE 1 (offset by the orbit)
510     X1=Xw+OffX1 : Z1=Zw+OffZ1
520     V1x(I)=INT(X1*H/Z1+Xc)
530     V1y(I)=INT(Yw*H/Z1+Yc)
540     CZ1(I)=Z1 : Zsum1=Zsum1+Z1
550     'CUBE 2 (on the opposite side of the orbit)
560     X2=Xw+OffX2 : Z2=Zw+OffZ2
570     V2x(I)=INT(X2*H/Z2+Xc)
580     V2y(I)=INT(Yw*H/Z2+Yc)
590     CZ2(I)=Z2 : Zsum2=Zsum2+Z2
600   NEXT I
610   REM ** Draw the farthest cube first (smaller average Z) **
620   IF Zsum1<Zsum2 GOTO 660
630     GOSUB 730 'Cube 1 first
640     GOSUB 870
650   GOTO 680
660     GOSUB 870 'Cube 2 first
670     GOSUB 730
680   'FPS counter every 10 s
690   FPS=FPS+1
700   IF TIME-T0>10 THEN  PRINT "FPS: ";STR$(FPS/10):T0=TIME:FPS=0
710 GOTO 310
720 REM ** Draw cube 1 (arrays V1x, V1y) **
730 FOR C=1 TO 6
740   Ax=V1x(F(1,C)) : Ay=V1y(F(1,C))
750   Bx=V1x(F(2,C)) : By=V1y(F(2,C))
760   CX=V1x(F(3,C)) : CY=V1y(F(3,C))
770   Dx=V1x(F(4,C)) : Dy=V1y(F(4,C))
780   'Hidden-face removal
790   Q1=Ax-Dx : Q2=Ay-Dy : Q3=Ax-Bx : Q4=Ay-By
800   IF Q1*Q4-Q2*Q3>=0 THEN GOTO 840
810   INK C+2
820   FTRIANGLE Ax,Ay,Bx,By,CX,CY
830   FTRIANGLE CX,CY,Dx,Dy,Ax,Ay
840 NEXT C
850 RETURN
860 REM ** Draw cube 2 (arrays V2x, V2y) **
870 FOR C=1 TO 6
880   Ax=V2x(F(1,C)) : Ay=V2y(F(1,C))
890   Bx=V2x(F(2,C)) : By=V2y(F(2,C))
900   CX=V2x(F(3,C)) : CY=V2y(F(3,C))
910   Dx=V2x(F(4,C)) : Dy=V2y(F(4,C))
920   'Hidden-face removal
930   Q1=Ax-Dx : Q2=Ay-Dy : Q3=Ax-Bx : Q4=Ay-By
940   IF Q1*Q4-Q2*Q3>=0 THEN GOTO 980
950   INK C+18
960   FTRIANGLE Ax,Ay,Bx,By,CX,CY
970   FTRIANGLE CX,CY,Dx,Dy,Ax,Ay
980 NEXT C
990 RETURN
1000 REM ** Data **
1010 DATA -1, 1, 1, -1, -1, 1, 1, -1
1020 DATA 1, 1, -1, -1, 1, 1, -1, -1
1030 DATA 1, 1, 1, 1, -1, -1, -1, -1
1040 DATA 1, 2, 3, 4
1050 DATA 5, 6, 2, 1
1060 DATA 8, 7, 6, 5
1070 DATA 4, 3, 7, 8
1080 DATA 2, 6, 7, 3
1090 DATA 5, 1, 4, 8

