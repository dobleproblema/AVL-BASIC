2 REM Rotate a translucent picture.
5 Started=TIME
20 MODE 640 : RAD
30 BLOAD "assets/anime.png",Backdrop$
40 BLOAD "assets/anime-dance/frame3.png",Card$
50 W=WIDTH : H=HEIGHT : T=TIME
60 FOR F=0 TO 179
70   A=F*PI/90 : C=COS(A) : S=SIN(A)
80   Ux=250*C : Uy=250*S : Vx=-170*S : Vy=170*C
90   X0=W/2-Ux-Vx : Y0=H/2-Uy-Vy : X1=W/2+Ux-Vx : Y1=H/2+Uy-Vy
100  X2=W/2+Ux+Vx : Y2=H/2+Uy+Vy : X3=W/2-Ux+Vx : Y3=H/2-Uy+Vy
110  Opacity=0.55+0.32*S
120  TRECTANGLE Backdrop$,0,0,W,H
130  TTRIANGLE Card$,X0,Y0,0,0,X1,Y1,W,0,X2,Y2,W,H ALPHA Opacity
140  TTRIANGLE Card$,X0,Y0,0,0,X2,Y2,W,H,X3,Y3,0,H ALPHA Opacity
150  FRAME
160 NEXT F
170 Finished=TIME
180 PRINT "Animation:";Finished-T;" s"
190 PRINT "Total:";Finished-Started;" s"
200 SCREEN CLOSE
