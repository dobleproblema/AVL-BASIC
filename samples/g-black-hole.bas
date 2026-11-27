100 REM BLACK HOLE - A physically faithful view of the sky around a non-rotating black hole.
110 REM Light follows Schwarzschild geometry. The observer is static at 120 GM/c^2.
120 MODE 960 : RAD : SMALLFONT TRANSPARENT : RANDOMIZE 1702
130 W=WIDTH : H=HEIGHT : CX=W/2 : CY=H/2 : Focal=0.625*W
140 Observer=120 : Metric=1-2/Observer
150 Critical=SQR(27*Metric)/Observer : Shadow=Focal*Critical/SQR(1-Critical*Critical)
160 TW=2048 : TH=1024 : Stride=2*TW : Black=Stride*TH
170 DIM Texel(Black+TW-1),SkyAt(W-1,H-1),Band(TW-1)
180 GOSUB PaintSky : GOSUB MapRays
190 Started=TIME
200 WHILE INKEY$<>CHR$(27)
210   REM Rotate the distant sky at a constant 24 texels per second.
220   Drift=ROUND(24*(TIME-Started)) MOD TW
225   Yaw=(TW+96-Drift) MOD TW
230   FOR Y=0 TO H-1
240     FOR X=0 TO W-1
250       PLOT X,Y,Texel(SkyAt(X,Y)+Yaw)
260     NEXT X
270   NEXT Y
280   MOVE W-100,22 : INK RGB(145,161,190) : LABEL "ESC to exit"
290   FRAME 30
300 WEND
310 Finished:
320 SCREEN CLOSE : END

330 PaintSky:
340 REM An all-sky longitude/latitude map, with a tilted band of stars and dust.
350 CLG OFFSCREEN
360 FOR U=0 TO TW-1
370   Lon=(U/TW-0.5)*2*PI : Band(U)=TH*(0.5+0.13*SIN(Lon))
380   Clouds=0.72+0.19*COS(3*Lon)+0.09*COS(11*Lon+0.8)
390   Rift=Band(U)+13*SIN(7*Lon+0.6) : Warm=0.5+0.5*SIN(2*Lon+0.4)
400   FOR V=0 TO TH-1
410     Fog=MAX(0,1-ABS(V-Band(U))/(0.105*TH))^2*Clouds
420     Fog=Fog*(1-0.52*MAX(0,1-ABS(V-Rift)/(0.018*TH)))
430     C=RGB(5+(22+15*Warm)*Fog,7+(28-5*Warm)*Fog,16+(53-15*Warm)*Fog)
440     K=V*Stride+U : Texel(K)=C : Texel(K+TW)=C
450   NEXT V
460   IF INKEY$=CHR$(27) THEN GOTO Finished
470 NEXT U
480 FOR I=1 TO 12500
490   U=INT(RND*TW) : V=INT((0.5+ASN(2*RND-1)/PI)*TH)
500   IF RND<0.61 THEN V=INT(Band(U)+0.15*TH*(RND+RND+RND-1.5))
510   Light=39+211*RND^2.6 : Warm=RND
520   C=RGB(Light*(0.63+0.37*Warm),Light*(0.92-0.09*Warm),Light*(1-0.31*Warm))
530   IF V>=0 AND V<TH THEN K=V*Stride+U : Texel(K)=C : Texel(K+TW)=C
540 NEXT I
550 REM Longitude and latitude offsets in texels, angular radius in radians, RGB.
560 FOR StarNo=1 TO 4
570   READ DU,DV,Star,Red,Green,Blue
580   GOSUB PaintStar
590 NEXT StarNo
600 DATA 0,0,0.015,255,238,193
610 DATA 52,31,0.008,200,226,255
620 DATA -48,-23,0.007,246,249,255
630 DATA -98,57,0.0075,255,218,166
670 RETURN

680 MapRays:
690 REM Symmetry makes the bending angle a function of image radius alone.
700 REM Tabulate it, with individual ray solves close to the capture boundary.
710 Q0=INT(Shadow*Shadow)+1 : Q1=INT((Shadow+5)^2)+1
720 RMax=INT(4*SQR(CX*CX+CY*CY))+1
730 DIM CloseBeta(Q1),BetaTab(RMax)
740 FOR Q=Q0 TO Q1
750   Rho=SQR(Q) : GOSUB RayAngle : CloseBeta(Q)=Beta
760 NEXT Q
770 FOR J=INT(4*SQR(Q1)) TO RMax
780   Rho=J/4 : GOSUB RayAngle : BetaTab(J)=Beta
790 NEXT J
800 FOR Y=0 TO H-1
810   DY=Y-CY
820   FOR X=0 TO W-1
830     DX=X-CX : Q=DX*DX+DY*DY
840     IF Q<Q0 THEN
850       SkyAt(X,Y)=Black
860     ELSE
870       Rho=SQR(Q)
880       IF Q<=Q1 THEN Beta=CloseBeta(Q) ELSE J=INT(4*Rho) : Beta=BetaTab(J)+(4*Rho-J)*(BetaTab(J+1)-BetaTab(J))
890       Spread=SIN(Beta)/Rho : SX=Spread*DX : SY=Spread*DY : SZ=COS(Beta)
900       IF SZ=0 THEN Lon=SGN(SX)*PI/2 ELSE Lon=ATN(SX/SZ)
910       IF SZ<0 THEN Lon=Lon+PI
920       U=(TW+ROUND((0.5+Lon/(2*PI))*TW)) MOD TW
930       V=MIN(TH-1,MAX(0,ROUND((0.5+ASN(MAX(-1,MIN(1,SY)))/PI)*TH)))
940       SkyAt(X,Y)=V*Stride+U
950     END IF
960   NEXT X
970   IF INKEY$=CHR$(27) THEN GOTO Finished
980 NEXT Y
990 RETURN

1000 RayAngle:
1010 REM u=1/r. The outer turning point solves u^2*(1-2*u)=1/b^2.
1020 B=Observer*Rho/SQR((Focal*Focal+Rho*Rho)*Metric)
1030 InvB2=1/(B*B) : Lo=1/Observer : Hi=1/3
1040 FOR RootStep=1 TO 44
1050   Turn=(Lo+Hi)/2
1060   IF Turn*Turn*(1-2*Turn)<InvB2 THEN Lo=Turn ELSE Hi=Turn
1070 NEXT RootStep
1080 A0=2-6*Turn : A2=6*Turn-1
1090 REM Add the angular sweep from the turning point to the observer and to infinity.
1100 Limit=SQR(1-1/(Observer*Turn)) : GOSUB Sweep : NearAngle=Integral
1110 Limit=1 : GOSUB Sweep : Beta=PI-NearAngle-Integral
1120 RETURN

1130 Sweep:
1140 REM u=Turn*(1-t^2), then t=sqrt(A0)*sinh(z), regularizes the integral.
1150 N=64 : S=Limit/SQR(A0) : StepZ=LOG(S+SQR(1+S*S))/N : Integral=0
1160 FOR Sample=0 TO N
1170   ExpZ=EXP(Sample*StepZ) : T2=A0*((ExpZ-1/ExpZ)/2)^2
1180   Term=2*SQR((A0+T2)/(A0+A2*T2-2*Turn*T2*T2))
1190   IF Sample=0 OR Sample=N THEN Weight=1 ELSE Weight=2+2*(Sample MOD 2)
1200   Integral=Integral+Weight*Term
1210 NEXT Sample
1220 Integral=Integral*StepZ/3
1230 RETURN

1240 PaintStar:
1250 REM Sample a spherical disk, brighter at the center than at the limb.
1260 U0=TW/2+DU : V0=TH/2+DV : Lat0=DV*PI/TH : Limb=COS(Star)
1270 HalfU=INT(TW*ASN(SIN(Star)/COS(Lat0))/(2*PI))+1 : HalfV=INT(TH*Star/PI)+1
1280 FOR V=V0-HalfV TO V0+HalfV
1290   Lat=(V/TH-0.5)*PI
1300   FOR U=U0-HalfU TO U0+HalfU
1310     Direction=SIN(Lat0)*SIN(Lat)+COS(Lat0)*COS(Lat)*COS((U-U0)*2*PI/TW)
1320     IF Direction>=Limb THEN
1330       Light=0.44+0.56*SQR(MAX(0,1-(ACS(MIN(1,Direction))/Star)^2))
1340       C=RGB(Red*Light,Green*Light,Blue*Light)
1350       K=V*Stride+U : Texel(K)=C : Texel(K+TW)=C
1360     END IF
1370   NEXT U
1380 NEXT V
1390 RETURN
