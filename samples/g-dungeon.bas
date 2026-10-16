10 ' A first-person maze with textured walls, music and sound effects.
20 ' Find the key, open the gate and reach the escape portal.
30 ' W/S or UP/DOWN walks; LEFT/RIGHT turns; A/D strafes; SPACE runs; E opens the gate.
40 ' M map; Q quality; F FPS limit; R restart; H HUD; B music; N mute; ESC exit.
50 SCREEN : MODE 640 : RAD : PAPER 0 : CLG : MAT BASE 0
60 N=19 : Cols=640 : Cap=1 : MapOn=0 : Hud=1 : Radius=0.18
70 Plane=0.66 : Proj=320/Plane : Far=18 : Nt=12 : No=14
80 DIM W(N-1,N-1),Seen(N-1,N-1),Light(N-1,N-1),Rows$(N-1)
90 DIM Ox(No-1),Oy(No-1),Wnx(Nt-1),Wny(Nt-1),Kind(No-1),Order(No-1),Od(No-1),ZBuf(639)
100 DIM Art(2,31,15),Pal(10),Br(3071),Bg(3071),Bb(3071)
110 DIM D(639,479),CamX(N,N),CamZ(N,N),Glow(N,N)
120 DIM Ax(3),Az(3),Au(3),Av(3),Al(3),Bx(3),Bz(3),Bu(3),Bv(3),Bl(3)
130 ' Shared state: maze/player/audio, immutable textures and reusable clipping buffers.
140 DIM Fx(3),Fy(3),Fq(3),Fr(3),Fg(3),Fb(3)
145 DIM MaskRuns(2,15),MaskRunStart(2,15,31),MaskRunEnd(2,15,31),MaskRunColor(2,15,31)
150 SMALLFONT TRANSPARENT : INK RGB(224,201,144)
160 LOCATE 3,2 : GPRINT "AVL DUNGEON - CARVING STONE AND LIGHT..." : FRAME
170 T=0 : CALL loadMaze : CALL buildWallTextures : CALL buildObjectArt(Art,Pal) : CALL buildMaskRuns(Art,MaskRuns,MaskRunStart,MaskRunEnd,MaskRunColor) : CALL loadAudio : CALL resetPlay
180 Last=TIME : TLast=TIME : Fps=0 : Count=0 : Frames=0 : T=0
190 ' Main loop; elapsed-time motion and bounded collision substeps.
200 WHILE 1
210   Now=TIME : Dt=MAX(0.001,MIN(0.05,Now-Last)) : Last=Now : T=T+Dt
220   K$=UPPER$(INKEY$)
230   IF K$=CHR$(27) OR KEYDOWN(27) THEN EXIT WHILE
240   IF K$="M" THEN MapOn=1-MapOn
250   IF K$="H" THEN Hud=1-Hud
260   IF K$="F" THEN Cap=1-Cap
270   IF K$="Q" THEN Cols=Cols*2:IF Cols>640 THEN Cols=160
280   IF K$="R" THEN
290     CALL resetPlay : Last=TIME
300   ELSE
310     CALL handleAudioKeys
320     Cw=640/Cols
330     Turn=ABS(KEYDOWN(29))-ABS(KEYDOWN(28))
340     Walk=MAX(ABS(KEYDOWN(87)),ABS(KEYDOWN(30)))-MAX(ABS(KEYDOWN(83)),ABS(KEYDOWN(31)))
350     Strafe=ABS(KEYDOWN(68))-ABS(KEYDOWN(65)) : Running=ABS(KEYDOWN(32))
360     IF Won THEN Turn=0:Walk=0:Strafe=0
370     Ang=Ang+Turn*Dt*1.9 : Ang=Ang-2*PI*INT(Ang/(2*PI))
380     Dx=COS(Ang) : Dy=SIN(Ang) : Plx=-Dy*Plane : Ply=Dx*Plane
390     CALL movePlayer : CALL tryGate
400     Hor=240+SIN(Pace)*2 : Tr=TIME : CLG OFFSCREEN
410     CALL drawPlanes : CALL drawWalls : CALL drawObjects : CALL drawLantern
420     IF MapOn THEN CALL drawMinimap
430     RenderMs=0.85*RenderMs+150*(TIME-Tr)
440     CALL drawHud
450     IF Cap THEN FRAME 60 ELSE FRAME
460     Count=Count+1 : Frames=Frames+1
470     IF TIME-TLast>=0.5 THEN Fps=Count/(TIME-TLast):Count=0:TLast=TIME
480   ENDIF
490 WEND
500 AUDIO STOP : SCREEN CLOSE
510 END
520 ' Circle movement, normalized diagonals, separate-axis sliding
530 DEF SUB movePlayer
540   LOCAL Speed, Mx, My, Steps, Mstep, OldX, OldY, Nx
550   LOCAL Ny
560   Speed=2.2+Running*1.3 : Mx=(Dx*Walk-Dy*Strafe)*Speed*Dt : My=(Dy*Walk+Dx*Strafe)*Speed*Dt
570   IF Walk<>0 AND Strafe<>0 THEN Mx=Mx*0.70710678118655:My=My*0.70710678118655
580   Steps=MAX(1,INT(MAX(ABS(Mx),ABS(My))/0.08)+1) : Mx=Mx/Steps : My=My/Steps
590   OldX=Px : OldY=Py
600   FOR Mstep=1 TO Steps
610     Nx=Px+Mx : IF FNCanOccupy(W,Nx,Py) THEN Px=Nx
620     Ny=Py+My : IF FNCanOccupy(W,Px,Ny) THEN Py=Ny
630   NEXT Mstep
640   Moved=SQR((Px-OldX)^2+(Py-OldY)^2) : Pace=Pace+Moved*7
650   CALL updateFootsteps
660   Seen(INT(Py),INT(Px))=1
670   IF Have=0 AND (Px-Ox(12))^2+(Py-Oy(12))^2<0.3 THEN
680     Have=1:Msg$="GOLD KEY FOUND - FIND THE LOCKED GATE":MsgT=T+4:CALL playKey
690   ENDIF
700   IF Won=0 AND Opened AND (Px-Ox(13))^2+(Py-Oy(13))^2<0.28 THEN
710     Won=1:Msg$="YOU ESCAPED AVL DUNGEON!  PRESS R TO EXPLORE AGAIN":MsgT=T+99999:CALL playVictory
720   ENDIF
730 SUBEND
740 ' Exact circle-to-solid-tile test; whole World array is passed by reference.
750 DEF FNCanOccupy(World,PosX,PosY)
760   LOCAL Cx,Cy,Qx,Qy
770   FNCanOccupy=0
780   IF PosX<Radius OR PosY<Radius OR PosX>N-Radius OR PosY>N-Radius THEN EXIT FN
790   FOR Cy=INT(PosY-Radius) TO INT(PosY+Radius)
800     FOR Cx=INT(PosX-Radius) TO INT(PosX+Radius)
810       IF World(Cy,Cx)<>0 THEN
820         Qx=MAX(Cx,MIN(Cx+1,PosX)) : Qy=MAX(Cy,MIN(Cy+1,PosY))
830         IF (PosX-Qx)^2+(PosY-Qy)^2<Radius*Radius THEN EXIT FN
840       ENDIF
850     NEXT Cx
860   NEXT Cy
870   FNCanOccupy=1
880 FNEND
890 ' The key is collected by proximity; E opens only the nearby gate
900 DEF SUB tryGate
910   LOCAL Dd, Facing
920   IF K$<>"E" OR Opened OR Won THEN EXIT SUB
930   Dd=(Px-14.5)^2+(Py-15.5)^2 : Facing=(14.5-Px)*Dx+(15.5-Py)*Dy
940   IF Dd>2.25 OR Facing<0.15 THEN Msg$="MOVE CLOSER AND FACE THE LOCKED GATE":MsgT=T+2:EXIT SUB
950   IF Have=0 THEN Msg$="THE GATE NEEDS THE GOLD KEY":MsgT=T+3:CALL playLatch:EXIT SUB
960   W(15,14)=0 : Opened=1 : Msg$="GATE UNLOCKED - REACH THE BLUE PORTAL" : MsgT=T+4 : CALL playGate
970 SUBEND
980 ' Reset play state; generated textures and display settings are retained
990 DEF SUB resetPlay
1000   Px=2.5 : Py=2.5 : Ang=0 : Pace=0 : Have=0 : Opened=0 : Won=0
1010   MAT Seen=0 : W(15,14)=3 : RenderMs=0
1020   Msg$="FIND THE GOLD KEY, UNLOCK THE GATE, REACH THE PORTAL" : MsgT=T+4
1030   AUDIO STOP : FootDist=0 : Foot=0 : LockUntil=0 : CALL syncMusic
1040 SUBEND
1050 ' Maze layout: nine rooms, loops, pillars and a sealed exit room
1060 DEF SUB loadMaze
1070   LOCAL Y, X, I, A$
1080   RESTORE 1260
1090   FOR Y=0 TO N-1
1100     READ Rows$(Y)
1110     FOR X=0 TO N-1
1120       A$=MID$(Rows$(Y),X+1,1) : W(Y,X)=0
1130       IF A$="#" THEN
1140         W(Y,X)=1 : IF (X*7+Y*3) MOD 11<3 THEN W(Y,X)=2
1150       ENDIF
1160       IF A$="D" THEN W(Y,X)=3
1170       IF A$="K" THEN Ox(12)=X+0.5:Oy(12)=Y+0.5
1180     NEXT X
1190   NEXT Y
1200   RESTORE 1340
1210   FOR I=0 TO Nt-1 : READ Ox(I),Oy(I),Wnx(I),Wny(I) : Kind(I)=0 : NEXT I
1220   Kind(12)=1 : Ox(13)=17.5 : Oy(13)=16.5 : Kind(13)=2
1230   CALL buildLighting(Light,Glow,Ox,Oy)
1240 SUBEND
1250 ' Move the single K in this map to reposition the gold key.
1260 DATA "###################","#.....##.....##...#","#.................#"
1270 DATA "#.....##.....##...#","#.....###.#####...#","##.######.######.##"
1280 DATA "##.###.....#####.##","#...##..#..##.....#","#..............#..#"
1290 DATA "#...##.....##..#.K#","#...#####.###..#..#","#...#####.###.....#"
1300 DATA "##.######.#...#####","##.#####....#######","#.....##....###...#"
1310 DATA "#.............D...#","#.....##....###..E#","#.....##....###...#"
1320 DATA "###################"
1330 ' Wall torch centres and outward normals; 0.04 in front of solid stone
1340 DATA 3.5,1.04,0,1,5.96,3.5,-1,0,10.5,1.04,0,1,11.5,3.96,0,-1
1350 DATA 17.96,2.5,-1,0,1.04,9.5,1,0,7.5,6.04,0,1,17.96,8.5,-1,0
1360 DATA 16.5,11.96,0,-1,3.5,17.96,0,-1,9.5,17.96,0,-1,16.5,14.04,0,1
1370 ' Stone/moss/iron-bound oak base texels; gentle detail limits shimmer
1380 DEF SUB buildWallTextures
1390   LOCAL M, Y, X, I, By, Bx, Grain, Var
1400   LOCAL R, G, B, L, Tile, Ax, Ay, Shade
1410   LOCAL C
1420   FOR M=0 TO 2
1430     FOR Y=0 TO 31
1440       FOR X=0 TO 31
1450         I=M*1024+Y*32+X : By=Y\8 : Bx=(X+(By MOD 2)*8) MOD 16
1460         Grain=3*SIN(X*0.8+Y*0.6)+2*COS(Y*0.9-X*0.4)
1470         Var=((X+(By MOD 2)*8)\16*7+By*11) MOD 13-6
1480         R=105+Var+Grain : G=116+Var+Grain : B=124+Var+Grain
1490         IF Bx<1 OR Y MOD 8<1 THEN R=39:G=44:B=51
1500         IF Bx=1 OR Y MOD 8=1 THEN R=R+15:G=G+15:B=B+15
1510         IF M=1 THEN R=R*0.83:G=G*0.94:B=B*0.82
1520         IF M=2 THEN
1530           R=97+10*SIN(X*0.8)+Grain : G=60+5*SIN(X*0.8)+Grain : B=32+Grain
1540           IF X MOD 8=0 THEN R=44:G=29:B=20
1550           IF Y=6 OR Y=7 OR Y=24 OR Y=25 THEN R=65:G=73:B=79
1560           IF (X=3 OR X=27) AND (Y=7 OR Y=24) THEN R=190:G=156:B=88
1570         ENDIF
1580         Br(I)=MAX(0,R) : Bg(I)=MAX(0,G) : Bb(I)=MAX(0,B)
1590       NEXT X
1600     NEXT Y
1610   NEXT M
1620   ' One immutable variable holds a 256 x 384 prelit texture atlas.
1630   ' Simple ATLAS$ enables the interpreter 's decoded texture cache.
1640   FOR M=0 TO 2
1650     FOR L=0 TO 31
1660       Tile=M*32+L : Ax=(Tile MOD 8)*32 : Ay=(Tile\8)*32 : Shade=0.11+L*0.89/31
1670       FOR Y=0 TO 31
1680         FOR X=0 TO 31
1690           I=M*1024+Y*32+X : C=RGB(INT(Br(I)*Shade),INT(Bg(I)*Shade),INT(Bb(I)*Shade))
1700           PLOT Ax+X,Ay+Y,C
1710         NEXT X
1720       NEXT Y
1730     NEXT L
1740   NEXT M
1750   Atlas$=SPRITE$(0,0,255,383)
1760   CALL buildPlaneTextures
1770 SUBEND
1780 ' Tiny pixel masks: wall-mounted torch, gold key, cyan escape portal
1790 DEF SUB buildObjectArt(Masks,Palette)
1800   LOCAL Y, X, D2
1810   MAT Masks=-1
1820   Palette(0)=RGB(42,44,48) : Palette(1)=RGB(108,94,65) : Palette(2)=RGB(188,118,44)
1830   Palette(3)=RGB(248,87,19) : Palette(4)=RGB(255,161,27) : Palette(5)=RGB(255,234,127)
1840   Palette(6)=RGB(236,179,57) : Palette(7)=RGB(255,231,123) : Palette(8)=RGB(20,99,141)
1850   Palette(9)=RGB(49,205,226) : Palette(10)=RGB(191,250,236)
1860   FOR Y=1 TO 10 : FOR X=5 TO 10 : Masks(0,Y,X)=0 : IF X=5 OR Y=10 THEN Masks(0,Y,X)=1
1870   NEXT X : NEXT Y
1880   FOR Y=7 TO 16 : FOR X=7 TO 8 : Masks(0,Y,X)=1
1890   NEXT X : NEXT Y
1900   FOR Y=14 TO 17 : FOR X=3 TO 12 : Masks(0,Y,X)=2 : NEXT X : NEXT Y
1910   FOR Y=17 TO 30
1920     FOR X=1 TO 14
1930       D2=((X-7.5)/6)^2+((Y-23)/8)^2
1940       IF D2<=1 THEN
1950         Masks(0,Y,X)=3 : IF D2<0.55 THEN Masks(0,Y,X)=4:IF D2<0.2 THEN Masks(0,Y,X)=5
1960       ENDIF
1970     NEXT X
1980   NEXT Y
1990   FOR Y=10 TO 25 : FOR X=0 TO 15
2000       D2=(X-4.5)^2+(Y-20)^2 : IF D2>=8 AND D2<21 THEN Masks(1,Y,X)=7
2010       IF (Y=19 OR Y=20) AND X>=8 THEN Masks(1,Y,X)=6
2020       IF (X=11 OR X=14) AND Y>=15 AND Y<=19 THEN Masks(1,Y,X)=7
2030   NEXT X : NEXT Y : CALL buildPortalArt(Masks)
2040 SUBEND
2050 ' One unlit texture per plane; TQUAD supplies the vertex lighting.
2060 DEF SUB buildPlaneTextures
2070   LOCAL Y, X, Grain, Luma
2080   FOR Y=0 TO 31 : FOR X=0 TO 31
2090       Grain=3*SIN(X*0.6+Y*0.9) : Luma=93+Grain+((X\16+Y\16) MOD 2)*8
2100       IF X MOD 16=0 OR Y MOD 16=0 THEN Luma=56
2110       PLOT X,Y,RGB(INT(Luma),INT(Luma*1.02),INT(Luma*1.03))
2120       Luma=65+Grain : IF (X+(Y\8 MOD 2)*8) MOD 16=0 OR Y MOD 8=0 THEN Luma=39
2130       PLOT X+40,Y,RGB(INT(Luma*0.75),INT(Luma*0.84),INT(Luma))
2140   NEXT X : NEXT Y
2150   Floor$=SPRITE$(0,0,31,31) : Ceil$=SPRITE$(40,0,71,31)
2160 SUBEND
2170 DEF SUB buildPortalArt(Masks)
2180   LOCAL Y, X, D2
2190   FOR Y=1 TO 30 : FOR X=0 TO 15
2200       D2=((X-7.5)/7)^2+((Y-16)/14)^2
2210       IF D2>0.75 AND D2<=1 THEN Masks(2,Y,X)=9
2220       IF D2>0.86 AND D2<0.95 THEN Masks(2,Y,X)=10
2230       IF D2<=0.75 THEN Masks(2,Y,X)=8
2240       IF Y<12 AND D2<0.75 AND Y MOD 3=0 THEN Masks(2,Y,X)=10
2250   NEXT X : NEXT Y
2260 SUBEND
2270 ' Fixed world tiles keep texture and torch light in the same place.
2271 ' Solid cells need no planes; wall/floor rounding can change a few edge pixels.
2280 DEF SUB drawPlanes
2290   LOCAL Y, X, Cx, Cz, Near, Pad, Edge, Flicker
2300   LOCAL Half
2310   FRECTANGLE 0,40,639,439,RGB(9,13,19) : MAT D=0
2320   Near=0.1 : Pad=SQR(0.5) : Edge=Pad*SQR(1+Plane^2) : Flicker=1+0.045*SIN(T*10)+0.025*SIN(T*17)
2330   FOR Y=0 TO N : FOR X=0 TO N
2340       CamX(Y,X)=-(X-Px)*Dy+(Y-Py)*Dx : CamZ(Y,X)=(X-Px)*Dx+(Y-Py)*Dy
2350   NEXT X : NEXT Y
2360   FOR Y=0 TO N-1 : FOR X=0 TO N-1
2370       Cx=CamX(Y,X)+(Dx-Dy)*0.5 : Cz=CamZ(Y,X)+(Dx+Dy)*0.5
2380       IF W(Y,X)=0 AND Cz+Pad>=Near AND ABS(Cx)<=Plane*Cz+Edge THEN
2390         Ax(0)=CamX(Y,X) : Az(0)=CamZ(Y,X) : Au(0)=X*32 : Av(0)=Y*32 : Al(0)=Glow(Y,X)
2400         Ax(1)=CamX(Y,X+1) : Az(1)=CamZ(Y,X+1) : Au(1)=(X+1)*32 : Av(1)=Y*32 : Al(1)=Glow(Y,X+1)
2410         Ax(2)=CamX(Y+1,X+1) : Az(2)=CamZ(Y+1,X+1) : Au(2)=(X+1)*32 : Av(2)=(Y+1)*32 : Al(2)=Glow(Y+1,X+1)
2420         Ax(3)=CamX(Y+1,X) : Az(3)=CamZ(Y+1,X) : Au(3)=X*32 : Av(3)=(Y+1)*32 : Al(3)=Glow(Y+1,X)
2430         IF MAX(Az(0),Az(1),Az(2),Az(3))>=Near THEN
2440           IF MIN(Az(0),Az(1),Az(2),Az(3))>=Near THEN
2450             Bn=4 : MAT Bx=Ax : MAT Bz=Az : MAT Bu=Au : MAT Bv=Av : MAT Bl=Al
2460             CALL drawClippedPlane(Flicker)
2470           ELSE
2480             ' Clip the original triangles separately to preserve their lighting.
2490             FOR Half=0 TO 1 : CALL clipPlaneTriangle(Half,Near,Flicker) : NEXT Half
2500           ENDIF
2510         ENDIF
2520       ENDIF
2530   NEXT X : NEXT Y
2540 SUBEND
2550 ' Clip triangle 0,1,2 or 0,2,3 against the near plane.
2560 DEF SUB clipPlaneTriangle(Half,Near,Flicker)
2570   LOCAL K0, K1, K, Mix
2580   Bn=0 : K0=Half+2
2590   FOR K=0 TO 2
2600     K1=0 : IF K>0 THEN K1=Half+K
2610     IF (Az(K0)>=Near)<>(Az(K1)>=Near) THEN
2620       Mix=(Near-Az(K0))/(Az(K1)-Az(K0))
2630       Bx(Bn)=Ax(K0)+(Ax(K1)-Ax(K0))*Mix : Bz(Bn)=Near : Bu(Bn)=Au(K0)+(Au(K1)-Au(K0))*Mix : Bv(Bn)=Av(K0)+(Av(K1)-Av(K0))*Mix : Bl(Bn)=Al(K0)+(Al(K1)-Al(K0))*Mix : Bn=Bn+1
2640     ENDIF
2650     IF Az(K1)>=Near THEN
2660       Bx(Bn)=Ax(K1) : Bz(Bn)=Az(K1) : Bu(Bn)=Au(K1) : Bv(Bn)=Av(K1) : Bl(Bn)=Al(K1) : Bn=Bn+1
2670     ENDIF
2680     K0=K1 : NEXT K
2690   CALL drawClippedPlane(Flicker)
2700 SUBEND
2710 DEF SUB drawClippedPlane(Flicker)
2720   LOCAL J, Gl
2730   IF Bn<3 THEN EXIT SUB
2740   FOR J=0 TO Bn-1
2750     Fq(J)=1/Bz(J) : Fx(J)=320+Proj*Bx(J)*Fq(J) : Fy(J)=Hor-0.5*Proj*Fq(J) : Gl=Bl(J)*Flicker
2760     Fr(J)=255*(0.12+1.8*Gl) : Fg(J)=255*(0.14+1.2*Gl) : Fb(J)=255*(0.18+0.55*Gl)
2770   NEXT J
2780   IF Bn=4 THEN
2790     TQUAD Floor$,D,Fx(0),Fy(0),Fq(0),Bu(0),Bv(0),Fr(0),Fg(0),Fb(0),Fx(1),Fy(1),Fq(1),Bu(1),Bv(1),Fr(1),Fg(1),Fb(1),Fx(2),Fy(2),Fq(2),Bu(2),Bv(2),Fr(2),Fg(2),Fb(2),Fx(3),Fy(3),Fq(3),Bu(3),Bv(3),Fr(3),Fg(3),Fb(3)
2800     TQUAD Ceil$,D,Fx(0),2*Hor-Fy(0),Fq(0),Bu(0),Bv(0),Fr(0),Fg(0),Fb(0),Fx(1),2*Hor-Fy(1),Fq(1),Bu(1),Bv(1),Fr(1),Fg(1),Fb(1),Fx(2),2*Hor-Fy(2),Fq(2),Bu(2),Bv(2),Fr(2),Fg(2),Fb(2),Fx(3),2*Hor-Fy(3),Fq(3),Bu(3),Bv(3),Fr(3),Fg(3),Fb(3)
2810   ELSE
2820     TTRIANGLE Floor$,D,Fx(0),Fy(0),Fq(0),Bu(0),Bv(0),Fr(0),Fg(0),Fb(0),Fx(1),Fy(1),Fq(1),Bu(1),Bv(1),Fr(1),Fg(1),Fb(1),Fx(2),Fy(2),Fq(2),Bu(2),Bv(2),Fr(2),Fg(2),Fb(2)
2830     TTRIANGLE Ceil$,D,Fx(0),2*Hor-Fy(0),Fq(0),Bu(0),Bv(0),Fr(0),Fg(0),Fb(0),Fx(1),2*Hor-Fy(1),Fq(1),Bu(1),Bv(1),Fr(1),Fg(1),Fb(1),Fx(2),2*Hor-Fy(2),Fq(2),Bu(2),Bv(2),Fr(2),Fg(2),Fb(2)
2840   ENDIF
2850 SUBEND
2860 ' DDA rays use camera-plane distance: walls and sprites share one depth
2870 DEF SUB drawWalls
2880   LOCAL C, Camera, Rx, Ry, Gx, Gy, DelX, DelY
2890   LOCAL Sx, Sy, Sdx, Sdy, Dstep, Dist, Matl
2900   LOCAL Side, WallX, TexX, Gl, Shade, Sh, Tile, U
2910   LOCAL V, Lh, WLow, WHigh, Yb, Yt, Vb, Vt
2915   ' The maze keeps a solid border; every ray hits before N*2 steps.
2920   FOR C=0 TO Cols-1
2930     Camera=2*(C+0.5)/Cols-1 : Rx=Dx+Plx*Camera : Ry=Dy+Ply*Camera
2940     Gx=INT(Px) : Gy=INT(Py) : DelX=1E+30 : DelY=1E+30
2950     IF ABS(Rx)>1E-12 THEN DelX=ABS(1/Rx)
2960     IF ABS(Ry)>1E-12 THEN DelY=ABS(1/Ry)
2970     Sx=1 : Sy=1 : Sdx=(Gx+1-Px)*DelX : Sdy=(Gy+1-Py)*DelY
2980     IF Rx<0 THEN Sx=-1:Sdx=(Px-Gx)*DelX
2990     IF Ry<0 THEN Sy=-1:Sdy=(Py-Gy)*DelY
3010     FOR Dstep=1 TO N*2
3020       IF Sdx<Sdy THEN Sdx=Sdx+DelX:Gx=Gx+Sx:Side=0 ELSE Sdy=Sdy+DelY:Gy=Gy+Sy:Side=1
3040       Seen(Gy,Gx)=1
3050       Matl=W(Gy,Gx) : IF Matl>0 THEN EXIT FOR
3060     NEXT Dstep
3080     Dist=Sdx-DelX : IF Side=1 THEN Dist=Sdy-DelY
3090     Dist=MAX(0.001,Dist)
3130     ZBuf(C)=Dist : WallX=Py+Dist*Ry : IF Side=1 THEN WallX=Px+Dist*Rx
3140     WallX=WallX-INT(WallX) : TexX=INT(WallX*32)
3150     IF (Side=0 AND Rx>0) OR (Side=1 AND Ry<0) THEN TexX=31-TexX
3160     Gl=Light(Gy,Gx)
3170     Shade=(0.1+0.82/(1+Dist*0.2)+Gl*0.13) : IF Side=1 THEN Shade=Shade*0.82
3180     Sh=MAX(0,MIN(31,INT(Shade*31))) : Tile=(Matl-1)*32+Sh
3190     U=(Tile MOD 8)*32+TexX+0.5 : V=(Tile\8)*32 : Lh=Proj/Dist
3200     WLow=Hor-Lh*0.5 : WHigh=Hor+Lh*0.5 : Yb=MAX(40,WLow) : Yt=MIN(440,WHigh)
3210     Vb=V+(Yb-WLow)*32/Lh : Vt=V+(Yt-WLow)*32/Lh
3220     TRECTANGLE Atlas$,C*Cw,Yb,(C+1)*Cw,Yt,U,Vb,U,Vt
3230   NEXT C
3240 SUBEND
3250 ' Depth-sort objects; torches use wall planes, key and portal billboards
3260 DEF SUB drawObjects
3270   LOCAL I, Obj, Sk, Ddy, Ddx, Depth, Across
3280   CALL sortObjects(Order,Od,Ox,Oy)
3290   FOR Obj=0 TO No-1
3300     I=Order(Obj) : Sk=Kind(I)
3310     IF Sk<>1 OR Have=0 THEN
3320       Ddy=Oy(I)-Py : Ddx=Ox(I)-Px : Depth=Ddx*Dx+Ddy*Dy : Across=-Ddx*Dy+Ddy*Dx
3330       IF Sk=0 THEN CALL drawWallTorch(I,Ddx,Ddy,Depth,Across) ELSE CALL drawBillboard(Sk,Depth,Across)
3340     ENDIF
3350   NEXT Obj
3360 SUBEND
3370 ' Key and portal billboards; reject invisible objects before scanning their masks.
3379 ' Precalculated immutable mask runs preserve color order and transparent gaps.
3380 DEF SUB drawBillboard(Sk,Depth,Across)
3390   LOCAL Center,Sw,Sh,Z0,SLeft,Bottom,XLo,XHi, RunIndex
3400   LOCAL Sc,Tx,LastCol,StartY,Ty,Yb,Yt
3410   IF Depth<0.12 OR Depth>Far THEN EXIT SUB
3420   Center=320+Proj*Across/Depth
3430   IF Sk=1 THEN Sw=Proj*0.46/Depth:Sh=Proj*0.46/Depth:Z0=0.25+0.035*SIN(T*3) ELSE Sw=Proj*0.75/Depth:Sh=Proj*0.91/Depth:Z0=0.02
3440   SLeft=Center-Sw*0.5 : Bottom=Hor+(Z0-0.5)*Proj/Depth
3450   IF SLeft>639 OR SLeft+Sw<0 THEN EXIT SUB
3460   XLo=MAX(0,INT(SLeft/Cw)) : XHi=MIN(Cols-1,INT((SLeft+Sw)/Cw))
3470   FOR Sc=XLo TO XHi
3480     Tx=INT(((Sc+0.5)*Cw-SLeft)*16/Sw)
3490     IF Depth<ZBuf(Sc) AND Tx>=0 AND Tx<=15 THEN
3510       FOR RunIndex=0 TO MaskRuns(Sk,Tx)-1
3520         StartY=MaskRunStart(Sk,Tx,RunIndex) : Ty=MaskRunEnd(Sk,Tx,RunIndex) : LastCol=MaskRunColor(Sk,Tx,RunIndex)
3530         Yb=Bottom+StartY*Sh/32 : Yt=Bottom+Ty*Sh/32
3540         Yb=MAX(40,INT(Yb)) : Yt=MIN(439,INT(Yt))
3550         IF Yt>=Yb THEN FRECTANGLE Sc*Cw,Yb,(Sc+1)*Cw-1,Yt,Pal(LastCol)
3560       NEXT RunIndex
3620     ENDIF
3630   NEXT Sc
3640 SUBEND
3650 ' Wall-aligned torch planes, elevated above the floor on iron brackets
3659 ' Precalculated immutable mask runs preserve color order and transparent gaps.
3660 DEF SUB drawWallTorch(I,Ddx,Ddy,Depth,Across)
3670   LOCAL Pn, Vx, Vy, Tpd, Tpa, Td1, Tc1, Td2, RunIndex
3680   LOCAL Tc2, X1, X2, XLo, XHi, Flicker, Sc, Camera
3690   LOCAL Rx, Ry, Den, Td, Tx, Sh, Bottom, LastCol
3700   LOCAL StartY, Ty, Yb, Yt
3710   Pn=Ddx*Wnx(I)+Ddy*Wny(I) : IF Pn>=-0.015 THEN EXIT SUB
3720   Vx=-Wny(I) : Vy=Wnx(I) : Tpd=Vx*Dx+Vy*Dy : Tpa=-Vx*Dy+Vy*Dx
3730   Td1=Depth-0.12*Tpd : Tc1=Across-0.12*Tpa : Td2=Depth+0.12*Tpd : Tc2=Across+0.12*Tpa
3740   IF Td1<=0.08 AND Td2<=0.08 THEN EXIT SUB
3750   IF Td1<0.08 THEN Tc1=Tc1+(Tc2-Tc1)*(0.08-Td1)/(Td2-Td1):Td1=0.08
3760   IF Td2<0.08 THEN Tc2=Tc2+(Tc1-Tc2)*(0.08-Td2)/(Td1-Td2):Td2=0.08
3770   X1=320+Proj*Tc1/Td1 : X2=320+Proj*Tc2/Td2
3780   XLo=MAX(0,INT(MIN(X1,X2)/Cw)) : XHi=MIN(Cols-1,INT(MAX(X1,X2)/Cw))
3790   IF XLo>XHi THEN EXIT SUB
3800   Flicker=1+0.025*SIN(T*10+I*2.7)+0.018*SIN(T*17+I)
3810   FOR Sc=XLo TO XHi
3820     Camera=2*(Sc+0.5)/Cols-1 : Rx=Dx+Plx*Camera : Ry=Dy+Ply*Camera : Den=Rx*Wnx(I)+Ry*Wny(I)
3830     IF ABS(Den)>=1E-9 THEN
3840       Td=Pn/Den
3850       IF Td>=0.08 AND Td<ZBuf(Sc) AND Td<=Far THEN
3860         Tx=INT((0.5+((Px+Rx*Td-Ox(I))*Vx+(Py+Ry*Td-Oy(I))*Vy)/0.24)*16)
3870         IF Tx>=0 AND Tx<=15 THEN
3880           Sh=Proj*0.52/Td : Bottom=Hor-0.1*Proj/Td
3890           FOR RunIndex=0 TO MaskRuns(0,Tx)-1
3900             StartY=MaskRunStart(0,Tx,RunIndex) : Ty=MaskRunEnd(0,Tx,RunIndex) : LastCol=MaskRunColor(0,Tx,RunIndex)
3910             Yb=Bottom+StartY*Sh/32 : Yt=Bottom+Ty*Sh/32
3920             IF StartY>=17 THEN Yb=Bottom+(Yb-Bottom)*Flicker:Yt=Bottom+(Yt-Bottom)*Flicker
3930             Yb=MAX(40,INT(Yb)) : Yt=MIN(439,INT(Yt))
3940             IF Yt>=Yb THEN FRECTANGLE Sc*Cw,Yb,(Sc+1)*Cw-1,Yt,Pal(LastCol)
3950           NEXT RunIndex
4010         ENDIF
4020       ENDIF
4030     ENDIF
4040   NEXT Sc
4050 SUBEND
4060 ' Explored minimap; unseen rooms stay hidden until rays visit them
4070 DEF SUB drawMinimap
4080   LOCAL Y, X, Mc, Ax, Ay
4090   FRECTANGLE 508,310,635,437,RGB(7,14,20)
4100   RECTANGLE 508,310,635,437,RGB(105,127,120)
4110   FOR Y=0 TO N-1 : FOR X=0 TO N-1
4120       IF Seen(Y,X) THEN
4130         Mc=RGB(46,56,57) : IF W(Y,X)>0 THEN Mc=RGB(116,126,124)
4140         IF W(Y,X)=3 THEN Mc=RGB(155,101,42)
4150         FRECTANGLE 515+X*6,431-Y*6,519+X*6,427-Y*6,Mc
4160       ENDIF
4170   NEXT X : NEXT Y
4180   IF Have=0 AND Seen(INT(Oy(12)),INT(Ox(12))) THEN FCIRCLE 515+Ox(12)*6,432-Oy(12)*6,2,Pal(7)
4190   IF Seen(16,17) THEN FCIRCLE 518+17*6,429-16*6,2,Pal(9)
4200   Ax=515+Px*6 : Ay=432-Py*6 : FCIRCLE Ax,Ay,2,RGB(244,225,155)
4210   DRAW Ax,Ay,Ax+Dx*6,Ay-Dy*6,RGB(244,225,155)
4220 SUBEND
4230 ' Opaque brass, alpha glass and flame glow; the handle stays open
4240 DEF SUB drawLantern
4250   LOCAL Lx, Ly, Fh
4260   Lx=548+SIN(Pace)*2 : Ly=42+COS(Pace)*3
4270   FRECTANGLE Lx+5,Ly+10,Lx+38,Ly+60,RGB(85,139,144) ALPHA 0.18
4280   CIRCLE Lx+22,Ly+81,11,RGB(152,112,51) : CIRCLE Lx+22,Ly+81,10,RGB(213,163,75)
4290   FRECTANGLE Lx,Ly,Lx+43,Ly+9,RGB(182,134,60) : FRECTANGLE Lx,Ly+60,Lx+43,Ly+68,RGB(182,134,60)
4300   FRECTANGLE Lx,Ly+10,Lx+4,Ly+59,RGB(182,134,60) : FRECTANGLE Lx+39,Ly+10,Lx+43,Ly+59,RGB(182,134,60)
4310   FCIRCLE Lx+22,Ly+27,10,RGB(255,161,46),1.7 ALPHA 0.2
4320   Fh=45+SIN(T*11)*1.3 : FTRIANGLE Lx+13,Ly+23,Lx+31,Ly+23,Lx+22+SIN(T*7)*2,Ly+Fh,RGB(248,145,33) ALPHA 0.9
4330   FTRIANGLE Lx+18,Ly+23,Lx+26,Ly+23,Lx+22,Ly+36,RGB(255,234,150)
4340   FRECTANGLE Lx+20,Ly+9,Lx+24,Ly+18,RGB(198,158,89)
4350   FRECTANGLE Lx+3,Ly+4,Lx+40,Ly+8,RGB(240,190,103)
4360   FRECTANGLE Lx-4,Ly+61,Lx+47,Ly+67,RGB(213,163,75)
4370   FRECTANGLE Lx+18,Ly+10,Lx+20,Ly+59,RGB(181,138,61)
4380 SUBEND
4390 ' HUD and the win overlay are deliberately drawn after the world
4400 DEF SUB drawHud
4410   LOCAL Ks$, Fs$, As$
4420   FRECTANGLE 0,0,639,39,RGB(10,17,23) : FRECTANGLE 0,440,639,479,RGB(10,17,23)
4430   DRAW 0,439,639,439,RGB(124,105,63) : DRAW 0,40,639,40,RGB(124,105,63)
4440   IF Hud=0 AND Won=0 THEN EXIT SUB
4450   SMALLFONT TRANSPARENT : INK RGB(233,214,173)
4460   Ks$="--" : IF Have THEN Ks$="GOLD"
4470   Fs$="FREE" : IF Cap THEN Fs$="CAP60"
4480   As$="OFF" : IF Music AND Won=0 THEN As$="ON"
4490   IF Muted THEN As$="MUTE"
4500   LOCATE 2,0 : GPRINT "AVL DUNGEON  ";Cols;" COLS  KEY:";Ks$;"  ";USING "###.0";Fps;" FPS ";Fs$;"  AUDIO:";As$
4510   INK RGB(143,174,182) : LOCATE 2,1
4520   IF T<MsgT THEN GPRINT Msg$ ELSE GPRINT "RENDER ";USING "##.0";RenderMs;" ms  |  FIND THE KEY - OPEN THE GATE - ESCAPE"
4530   LOCATE 2,28 : GPRINT "W/S OR UP/DOWN WALK  LEFT/RIGHT TURN  A/D STRAFE  SPACE RUN  E GATE"
4540   LOCATE 2,29 : GPRINT "M MAP  Q QUALITY  F FPS  R RESTART  H HUD  B MUSIC  N MUTE  ESC EXIT"
4550   IF Won=0 THEN EXIT SUB
4560   FRECTANGLE 115,183,525,292,RGB(10,22,29) : RECTANGLE 115,183,525,292,Pal(9)
4570   BIGFONT TRANSPARENT : INK Pal(10) : LOCATE 14,13 : GPRINT "YOU ESCAPED!"
4580   SMALLFONT TRANSPARENT : INK RGB(232,208,147) : LOCATE 27,17 : GPRINT "R - RETURN TO THE DUNGEON"
4590 SUBEND
4600 ' Original PCM assets, loaded once; channel 1 music, 2 steps, 3-5 events
4610 DEF SUB loadAudio
4620   AUDIO ON : Music=1 : Muted=0
4630   AUDIO LOAD 1,"assets/audio/dungeon/lantern-path-loop.wav"
4640   AUDIO LOAD 2,"assets/audio/dungeon/step-stone-a.wav"
4650   AUDIO LOAD 3,"assets/audio/dungeon/step-stone-b.wav"
4660   AUDIO LOAD 4,"assets/audio/dungeon/key-chime.wav"
4670   AUDIO LOAD 5,"assets/audio/dungeon/gate-open.wav"
4680   AUDIO LOAD 6,"assets/audio/dungeon/victory-fanfare.wav"
4690   AUDIO LOAD 7,"assets/audio/dungeon/gate-locked.wav"
4700 SUBEND
4710 ' Distance-driven alternating footsteps; pressing against a wall is silent
4720 DEF SUB updateFootsteps
4730   IF Won OR Moved<0.000001 THEN EXIT SUB
4740   FootDist=FootDist+Moved
4750   IF FootDist<0.62 THEN EXIT SUB
4760   FootDist=FootDist-0.62 : Foot=1-Foot
4770   IF Muted THEN EXIT SUB
4780   AUDIO PLAY 2,2+Foot,0,0.4,(Foot*2-1)*0.1,1+Running*0.08
4790 SUBEND
4800 ' Audio preferences survive R; re-enabling output starts a fresh music loop
4810 DEF SUB handleAudioKeys
4820   IF K$="B" THEN Music=1-Music:CALL syncMusic
4830   IF K$="N" THEN Muted=1-Muted:CALL syncOutput
4840 SUBEND
4850 DEF SUB syncMusic
4860   IF Music=0 THEN AUDIO STOP 1:EXIT SUB
4870   IF Muted=0 AND Won=0 THEN AUDIO PLAY 1,1,1,0.7
4880 SUBEND
4890 DEF SUB syncOutput
4900   IF Muted THEN AUDIO OFF:EXIT SUB
4910   AUDIO ON : CALL syncMusic
4920 SUBEND
4930 DEF SUB playKey
4940   IF Muted=0 THEN AUDIO PLAY 3,4,0,0.85
4950 SUBEND
4960 DEF SUB playGate
4970   IF Muted=0 THEN AUDIO PLAY 4,5,0,0.85
4980 SUBEND
4990 DEF SUB playVictory
5000   AUDIO STOP 1 : AUDIO STOP 2
5010   IF Muted=0 THEN AUDIO PLAY 5,6,0,0.85
5020 SUBEND
5030 ' The stuck latch shares the gate channel; key repeat cannot stutter it.
5040 DEF SUB playLatch
5050   IF Muted OR T<LockUntil THEN EXIT SUB
5060   AUDIO PLAY 4,7,0,0.85 : LockUntil=T+0.5
5070 SUBEND
5080 ' Warm light is tied to fixed map coordinates, independent of the render grid.
5090 DEF SUB buildLighting(LightMap,VertexGlow,ObjectX,ObjectY)
5100   LOCAL Y,X,I,L,D2
5110   FOR Y=0 TO N-1 : FOR X=0 TO N-1
5120       L=0
5130       FOR I=0 TO Nt-1
5140         D2=(X+0.5-ObjectX(I))^2+(Y+0.5-ObjectY(I))^2 : L=MAX(L,1/(1+D2*1.6))
5150       NEXT I
5160       LightMap(Y,X)=L
5170   NEXT X : NEXT Y
5180   FOR Y=0 TO N : FOR X=0 TO N
5190       L=0 : FOR I=0 TO Nt-1 : L=MAX(L,1/(1+((X-ObjectX(I))^2+(Y-ObjectY(I))^2)*1.6)) : NEXT I
5200       VertexGlow(Y,X)=L
5210   NEXT X : NEXT Y
5220 SUBEND
5230 DEF SUB sortObjects(ObjectOrder,ObjectDistance,ObjectX,ObjectY)
5240   LOCAL I,J,Tmp
5250   FOR I=0 TO No-1 : ObjectOrder(I)=I : ObjectDistance(I)=(ObjectX(I)-Px)^2+(ObjectY(I)-Py)^2 : NEXT I
5260   FOR I=0 TO No-2 : FOR J=I+1 TO No-1
5270       IF ObjectDistance(ObjectOrder(I))<ObjectDistance(ObjectOrder(J)) THEN Tmp=ObjectOrder(I):ObjectOrder(I)=ObjectOrder(J):ObjectOrder(J)=Tmp
5280   NEXT J : NEXT I
5290 SUBEND
20000 ' Immutable mask runs retain the same transparent gaps, color order and Ty=32 flush.
20010 DEF SUB buildMaskRuns(Masks,Counts,Starts,Ends,Colors)
20020   LOCAL Sk,Tx,Ty,LastColor,StartY,MaskColor,RunIndex
20030   FOR Sk=0 TO 2 : FOR Tx=0 TO 15
20040       RunIndex=0 : LastColor=-1 : StartY=0
20050       FOR Ty=0 TO 32
20060         MaskColor=-1 : IF Ty<32 THEN MaskColor=Masks(Sk,Ty,Tx)
20070         IF MaskColor<>LastColor THEN
20080           IF LastColor>=0 THEN
20090             Starts(Sk,Tx,RunIndex)=StartY : Ends(Sk,Tx,RunIndex)=Ty : Colors(Sk,Tx,RunIndex)=LastColor : RunIndex=RunIndex+1
20100           ENDIF
20110           StartY=Ty : LastColor=MaskColor
20120         ENDIF
20130       NEXT Ty
20140       Counts(Sk,Tx)=RunIndex
20150   NEXT Tx : NEXT Sk
20160 SUBEND
