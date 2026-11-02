10 ' A widescreen first-person maze with textured walls and CPC chiptune music.
20 ' Find the key, open the gate and reach the escape portal.
30 ' W/S or UP/DOWN walks; LEFT/RIGHT turns; A/D strafes; SPACE runs; E opens the gate.
40 ' V resolution; U fullscreen; J window; M map; Q quality; F VSYNC; R restart; B music; N mute; ESC exit.
50 SCREEN : ScreenMode=960 : MODE ScreenMode : SCREEN FULL OFF : MOUSE OFF : RAD : PAPER 0 : CLG : MAT BASE 0
60 N=19 : Quality=2 : SyncEnabled=1 : SCREEN SYNC ON : MapOn=0 : Radius=0.18
70 Far=18 : Nt=12 : No=14
80 DIM W(N-1,N-1),Seen(N-1,N-1),Light(N-1,N-1),Rows$(N-1)
90 DIM Ox(No-1),Oy(No-1),Wnx(Nt-1),Wny(Nt-1),Kind(No-1),Order(No-1),Od(No-1),ZBuf(1279)
100 DIM Art(2,31,15),Pal(10),Br(3071),Bg(3071),Bb(3071)
110 DIM CamX(N,N),CamZ(N,N),Glow(N,N) : CALL configureDisplay
120 DIM Ax(3),Az(3),Au(3),Av(3),Al(3),Bx(3),Bz(3),Bu(3),Bv(3),Bl(3)
130 ' Shared state: maze/player/audio, immutable textures and reusable clipping buffers.
140 DIM Fx(3),Fy(3),Fq(3),Fr(3),Fg(3),Fb(3)
145 DIM MaskRuns(2,15),MaskRunStart(2,15,31),MaskRunEnd(2,15,31),MaskRunColor(2,15,31)
150 SMALLFONT TRANSPARENT : INK RGB(224,201,144)
160 LOCATE 3,2 : GPRINT "AVL DUNGEON CPC - CARVING STONE AND LIGHT..." : FRAME
170 T=0 : CALL loadMaze : CALL buildWallTextures : CALL buildObjectArt(Art,Pal) : CALL buildMaskRuns(Art,MaskRuns,MaskRunStart,MaskRunEnd,MaskRunColor) : CALL loadAudio : CALL resetPlay : GOSUB 5210 : GOSUB 5080
180 Last=TIME : TLast=TIME : Fps=0 : Count=0 : Frames=0 : T=0
190 ' Main loop; elapsed-time motion and bounded collision substeps.
200 WHILE 1
210   Now=TIME : Dt=MAX(0.001,MIN(0.05,Now-Last)) : Last=Now : T=T+Dt
220   K$=UPPER$(INKEY$)
230   IF K$=CHR$(27) OR KEYDOWN(27) THEN EXIT WHILE
240   IF K$="M" THEN MapOn=1-MapOn
250   IF K$="V" THEN CALL nextResolution
260   IF K$="F" THEN
261     IF SyncEnabled THEN SCREEN SYNC OFF ELSE SCREEN SYNC ON
262     SyncEnabled=1-SyncEnabled
263   ENDIF
265   CALL displayInput
270   IF K$="Q" THEN Quality=(Quality+1) MOD 3:Cols=CanvasW/(2^(2-Quality))
280   IF K$="R" THEN
290     CALL resetPlay : GOSUB 5210 : GOSUB 5080 : Last=TIME
300   ELSE
310     GOSUB 5030
320     Cw=CanvasW/Cols
330     Turn=ABS(KEYDOWN(29))-ABS(KEYDOWN(28))
340     Walk=MAX(ABS(KEYDOWN(87)),ABS(KEYDOWN(30)))-MAX(ABS(KEYDOWN(83)),ABS(KEYDOWN(31)))
350     Strafe=ABS(KEYDOWN(68))-ABS(KEYDOWN(65)) : Running=ABS(KEYDOWN(32))
360     IF Won THEN Turn=0:Walk=0:Strafe=0
370     Ang=Ang+Turn*Dt*1.9 : Ang=Ang-2*PI*INT(Ang/(2*PI))
380     Dx=COS(Ang) : Dy=SIN(Ang) : Plx=-Dy*Plane : Ply=Dx*Plane
390     CALL movePlayer : CALL tryGate
395     IF VictoryPending THEN GOSUB 5380:VictoryPending=0
400     Hor=CanvasH/2+SIN(Pace)*2*UiScale : Tr=TIME : CLG OFFSCREEN
410     CALL drawPlanes : CALL drawWalls : CALL drawObjects : CALL drawLantern
420     IF MapOn THEN CALL drawMinimap
430     RenderMs=0.85*RenderMs+150*(TIME-Tr)
440     CALL drawHud
450     FRAME
460     Count=Count+1 : Frames=Frames+1
470     IF TIME-TLast>=0.5 THEN Fps=Count/(TIME-TLast):Count=0:TLast=TIME
480   ENDIF
490 WEND
500 GOSUB 5210 : SCREEN CLOSE
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
710     Won=1:Msg$="YOU ESCAPED AVL DUNGEON!  PRESS R TO EXPLORE AGAIN":MsgT=T+99999:VictoryPending=1
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
1030   FootDist=0 : Foot=0 : LockUntil=0 : VictoryPending=0
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
2310   FRECTANGLE 0,ViewBottom,CanvasW-1,ViewTop,RGB(9,13,19) : MAT D=0
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
2750     Fq(J)=1/Bz(J) : Fx(J)=CanvasW/2+Proj*Bx(J)*Fq(J) : Fy(J)=Hor-0.5*Proj*Fq(J) : Gl=Bl(J)*Flicker
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
2900   LOCAL Side, WallX, TexX, Gl, Shade, SpriteH, Tile, U
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
3180     SpriteH=MAX(0,MIN(31,INT(Shade*31))) : Tile=(Matl-1)*32+SpriteH
3190     U=(Tile MOD 8)*32+TexX+0.5 : V=(Tile\8)*32 : Lh=Proj/Dist
3200     WLow=Hor-Lh*0.5 : WHigh=Hor+Lh*0.5 : Yb=MAX(ViewBottom,WLow) : Yt=MIN(ViewTop,WHigh)
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
3390   LOCAL Center,SpriteW,SpriteH,Z0,SLeft,Bottom,XLo,XHi, RunIndex
3400   LOCAL Sc,Tx,LastCol,StartY,Ty,Yb,Yt
3410   IF Depth<0.12 OR Depth>Far THEN EXIT SUB
3420   Center=CanvasW/2+Proj*Across/Depth
3430   IF Sk=1 THEN SpriteW=Proj*0.46/Depth:SpriteH=Proj*0.46/Depth:Z0=0.25+0.035*SIN(T*3) ELSE SpriteW=Proj*0.75/Depth:SpriteH=Proj*0.91/Depth:Z0=0.02
3440   SLeft=Center-SpriteW*0.5 : Bottom=Hor+(Z0-0.5)*Proj/Depth
3450   IF SLeft>CanvasW-1 OR SLeft+SpriteW<0 THEN EXIT SUB
3460   XLo=MAX(0,INT(SLeft/Cw)) : XHi=MIN(Cols-1,INT((SLeft+SpriteW)/Cw))
3470   FOR Sc=XLo TO XHi
3480     Tx=INT(((Sc+0.5)*Cw-SLeft)*16/SpriteW)
3490     IF Depth<ZBuf(Sc) AND Tx>=0 AND Tx<=15 THEN
3510       FOR RunIndex=0 TO MaskRuns(Sk,Tx)-1
3520         StartY=MaskRunStart(Sk,Tx,RunIndex) : Ty=MaskRunEnd(Sk,Tx,RunIndex) : LastCol=MaskRunColor(Sk,Tx,RunIndex)
3530         Yb=Bottom+StartY*SpriteH/32 : Yt=Bottom+Ty*SpriteH/32
3540         Yb=MAX(ViewBottom,INT(Yb)) : Yt=MIN(ViewTop,INT(Yt))
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
3690   LOCAL Rx, Ry, Den, Td, Tx, SpriteH, Bottom, LastCol
3700   LOCAL StartY, Ty, Yb, Yt
3710   Pn=Ddx*Wnx(I)+Ddy*Wny(I) : IF Pn>=-0.015 THEN EXIT SUB
3720   Vx=-Wny(I) : Vy=Wnx(I) : Tpd=Vx*Dx+Vy*Dy : Tpa=-Vx*Dy+Vy*Dx
3730   Td1=Depth-0.12*Tpd : Tc1=Across-0.12*Tpa : Td2=Depth+0.12*Tpd : Tc2=Across+0.12*Tpa
3740   IF Td1<=0.08 AND Td2<=0.08 THEN EXIT SUB
3750   IF Td1<0.08 THEN Tc1=Tc1+(Tc2-Tc1)*(0.08-Td1)/(Td2-Td1):Td1=0.08
3760   IF Td2<0.08 THEN Tc2=Tc2+(Tc1-Tc2)*(0.08-Td2)/(Td1-Td2):Td2=0.08
3770   X1=CanvasW/2+Proj*Tc1/Td1 : X2=CanvasW/2+Proj*Tc2/Td2
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
3880           SpriteH=Proj*0.52/Td : Bottom=Hor-0.1*Proj/Td
3890           FOR RunIndex=0 TO MaskRuns(0,Tx)-1
3900             StartY=MaskRunStart(0,Tx,RunIndex) : Ty=MaskRunEnd(0,Tx,RunIndex) : LastCol=MaskRunColor(0,Tx,RunIndex)
3910             Yb=Bottom+StartY*SpriteH/32 : Yt=Bottom+Ty*SpriteH/32
3920             IF StartY>=17 THEN Yb=Bottom+(Yb-Bottom)*Flicker:Yt=Bottom+(Yt-Bottom)*Flicker
3930         Yb=MAX(ViewBottom,INT(Yb)) : Yt=MIN(ViewTop,INT(Yt))
3940             IF Yt>=Yb THEN FRECTANGLE Sc*Cw,Yb,(Sc+1)*Cw-1,Yt,Pal(LastCol)
3950           NEXT RunIndex
4010         ENDIF
4020       ENDIF
4030     ENDIF
4040   NEXT Sc
4050 SUBEND
4060 ' Explored minimap; unseen rooms stay hidden until rays visit them
4070 DEF SUB drawMinimap
4080   LOCAL Y, X, Mc, Ax, Ay, MapX, MapY, Cell
4090   Cell=6*UiScale : MapX=CanvasW-125*UiScale : MapY=ViewTop-8*UiScale
4100   FRECTANGLE MapX-7*UiScale,MapY-122*UiScale,CanvasW-5*UiScale,ViewTop-3*UiScale,RGB(7,14,20) : RECTANGLE MapX-7*UiScale,MapY-122*UiScale,CanvasW-5*UiScale,ViewTop-3*UiScale,RGB(105,127,120)
4110   FOR Y=0 TO N-1 : FOR X=0 TO N-1
4120       IF Seen(Y,X) THEN
4130         Mc=RGB(46,56,57) : IF W(Y,X)>0 THEN Mc=RGB(116,126,124)
4140         IF W(Y,X)=3 THEN Mc=RGB(155,101,42)
4150       FRECTANGLE MapX+X*Cell,MapY-Y*Cell,MapX+X*Cell+4*UiScale,MapY-Y*Cell-4*UiScale,Mc
4160       ENDIF
4170   NEXT X : NEXT Y
4180   IF Have=0 AND Seen(INT(Oy(12)),INT(Ox(12))) THEN FCIRCLE MapX+Ox(12)*Cell,MapY+UiScale-Oy(12)*Cell,2*UiScale,Pal(7)
4190   IF Seen(16,17) THEN FCIRCLE MapX+3*UiScale+17*Cell,MapY-2*UiScale-16*Cell,2*UiScale,Pal(9)
4200   Ax=MapX+Px*Cell : Ay=MapY+UiScale-Py*Cell : FCIRCLE Ax,Ay,2*UiScale,RGB(244,225,155)
4210   DRAW Ax,Ay,Ax+Dx*Cell,Ay-Dy*Cell,RGB(244,225,155)
4220 SUBEND
4230 ' Opaque brass, alpha glass and flame glow; the handle stays open
4240 DEF SUB drawLantern
4250   LOCAL Lx, Ly, Fh
4255   ' Scale the brass and glow uniformly; this never stretches the 3D world.
4256   SCALE 0,(CanvasW-1)/UiScale,0,(CanvasH-1)/UiScale
4260   Lx=CanvasW/UiScale-92+SIN(Pace)*2 : Ly=ViewBottom/UiScale+2+COS(Pace)*3
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
4375   SCALE 0,CanvasW-1,0,CanvasH-1
4380 SUBEND
4390 ' HUD and the win overlay are deliberately drawn after the world
4400 DEF SUB drawHud
4410   LOCAL Ks$, SyncText$, As$, Info$
4420   FRECTANGLE 0,0,CanvasW-1,35,RGB(10,17,23) : FRECTANGLE 0,CanvasH-40,CanvasW-1,CanvasH-1,RGB(10,17,23)
4430   DRAW 0,35,CanvasW-1,35,RGB(124,105,63) : DRAW 0,CanvasH-40,CanvasW-1,CanvasH-40,RGB(124,105,63)
4450   SMALLFONT TRANSPARENT : INK RGB(233,214,173)
4460   Ks$="--" : IF Have THEN Ks$="GOLD"
4470   SyncText$="OFF" : IF SyncEnabled THEN SyncText$="ON"
4480   As$="OFF" : IF Music AND Won=0 THEN As$="ON"
4490   IF Muted THEN As$="MUTE"
4500   LOCATE 2,0 : GPRINT "DUNGEON CPC  ";CanvasW;"x";CanvasH;"  KEY:";Ks$
4510   Info$=STR$(INT(Fps+0.5))+" FPS  VSYNC:"+SyncText$+"  Q:"+STR$(Cols)+"  AUDIO:"+As$ : INK RGB(143,174,182) : LOCATE 2,1 : GPRINT Info$
4520   IF T<MsgT THEN MOVE 16,55:LABEL LEFT$(Msg$,CanvasW\8-4)
4530   MOVE 16,31 : LABEL "W/S WALK  ARROWS TURN/WALK  A/D STRAFE  SPACE RUN  E GATE  B MUSIC"
4540   MOVE 16,15 : LABEL "M MAP  Q QUALITY  F VSYNC  R RESTART  V RES  U FULL  J WINDOW  N MUTE  ESC"
4550   IF Won=0 THEN EXIT SUB
4560   FRECTANGLE CanvasW/2-205,CanvasH/2-55,CanvasW/2+205,CanvasH/2+55,RGB(10,22,29) : RECTANGLE CanvasW/2-205,CanvasH/2-55,CanvasW/2+205,CanvasH/2+55,Pal(9)
4570   BIGFONT TRANSPARENT : INK Pal(10) : LOCATE CanvasW/32-6,CanvasH/32-1 : GPRINT "YOU ESCAPED!"
4580   SMALLFONT TRANSPARENT : INK RGB(232,208,147) : LOCATE CanvasW/16-13,CanvasH/32+1 : GPRINT "R - RETURN TO THE DUNGEON"
4590 SUBEND
4610 ' CPC arrangement of The Lantern Path: 100 BPM, D dorian, 64 x 15cs.
4620 DEF SUB loadAudio
4630   LOCAL CpcI
4640   ' Melody and rhythm patterns are stored in the DATA below.
4650   ' A=lead, C=bass, B=percussion/FX. ON SQ feeds one note per callback.
4660   ' Music meets every four ticks; only initial/beat notes carry rendezvous bits.
4670   AUDIO ON : Music=1 : Muted=0 : CpcPlay=0 : CpcCelebrate=0
4680   CpcLn=33 : CpcBn=32 : CpcFxPri=0 : CpcFxUntil=0 : CpcDrumIdx=-1
4690   DIM CpcLp(32),CpcLd(32),CpcLEnv(32),CpcLSync(32),CpcBp(31),CpcBd(31),CpcDrum(15),CpcFp(7),CpcFd(7)
4700   RESTORE 5840 : FOR CpcI=0 TO CpcLn-1 : READ CpcLp(CpcI),CpcLd(CpcI),CpcLEnv(CpcI),CpcLSync(CpcI) : NEXT CpcI
4710   RESTORE 5930 : FOR CpcI=0 TO CpcBn-1 : READ CpcBp(CpcI),CpcBd(CpcI) : NEXT CpcI
4720   RESTORE 6010 : FOR CpcI=0 TO 15 : READ CpcDrum(CpcI) : NEXT CpcI
4730   RESTORE 6020 : FOR CpcI=0 TO 7 : READ CpcFp(CpcI),CpcFd(CpcI) : NEXT CpcI
4740   ' Soft lead articulation for one to four ticks; zero at each note 's tail.
4750   ENV 1,1,8,1,6,-1,2,1,-2,1
4760   ENV 2,1,8,1,6,-1,4,1,-2,1
4770   ENV 3,1,8,1,6,-1,7,1,-2,1
4780   ENV 4,1,8,1,6,-1,9,1,-2,1
4790   ENV 5,1,6,1,1,-1,10,1,-1,10,1,-1,17,1,-3,3
4800   ENT -1,1,1,4,2,-1,4,1,1,4
4810   ' Kick, snare and hat; only channel B uses the shared noise generator.
4820   ENV 6,1,6,1,6,-1,1 : ENT 2,6,90,1
4830   ENV 7,1,4,1,4,-1,1 : ENV 8,1,2,1,2,-1,1
4840   ' Alternating quiet shoe friction; no pitched footstep tone.
4850   ENV 9,1,3,1,3,-1,3 : ENV 10,1,3,1,3,-1,4
4860   ENV 11,1,11,1,10,-1,3,1,-1,3
4870   ENV 12,2,4,3,6,-1,9,1,-2,12 : ENT 3,6,42,7,=220,4,=1050,12
4880   ENV 13,1,8,1,8,-1,2 : ENT 4,3,55,2
4890   ENV 14,1,12,1,8,-1,10,1,-4,7
4900   ENV 15,1,6,1,5,-1,24,1,-1,31
4910 SUBEND
4920 ' Distance-driven footsteps replace percussion, but never important FX.
4930 DEF SUB updateFootsteps
4940   IF Won OR Moved<0.000001 THEN EXIT SUB
4950   FootDist=FootDist+Moved
4960   IF FootDist<0.62 THEN EXIT SUB
4970   FootDist=FootDist-0.62 : Foot=1-Foot
4980   IF Muted OR (CpcFxPri>1 AND TIME<CpcFxUntil) THEN EXIT SUB
4990   CpcFxPri=1 : CpcFxUntil=TIME+(10+Foot*3)/100
5000   SOUND 130,0,10+Foot*3,0,9+Foot,0,12+Foot*2+Running
5010 SUBEND
5020 ' These small GOSUB wrappers own DI/EI and ON SQ, which DEF SUB cannot execute.
5030 ' B toggles music; N toggles all audio. Preferences survive R.
5040 IF K$="B" THEN Music=1-Music:GOSUB 5080
5050 IF K$="N" THEN Muted=1-Muted:GOSUB 5130
5060 CALL updatePercussion
5070 RETURN
5080 IF Won THEN RETURN
5090 GOSUB 5160
5100 IF Music=0 OR Muted THEN RETURN
5110 DI : CpcLIdx=0 : CpcBIdx=0 : CpcDrumIdx=-1 : CpcStart=TIME+0.01 : CpcPlay=1
5120 GOSUB 5490 : GOSUB 5540 : EI : RETURN
5130 GOSUB 5210
5140 IF Muted THEN AUDIO OFF:RETURN
5150 AUDIO ON : GOSUB 5080 : RETURN
5160 ' Disarm before flushing A/C; leave an important effect on B intact.
5170 DI : CpcPlay=0 : ON SQ(1) GOSUB 0 : ON SQ(4) GOSUB 0
5180 SOUND 133,0,1,0
5190 IF CpcFxPri=0 THEN SOUND 130,0,1,0
5200 EI : RETURN
5210 ' Explicit cleanup for R, mute, victory and Esc; END alone keeps queues.
5220 DI : CpcPlay=0 : CpcCelebrate=0
5230 ON SQ(1) GOSUB 0 : ON SQ(2) GOSUB 0 : ON SQ(4) GOSUB 0
5240 SOUND 135,0,1,0 : CpcFxPri=0 : CpcFxUntil=0 : CpcDrumIdx=-1
5250 EI : RETURN
5260 ' Key: D6/F6/A6 reward arpeggio, above steps and the stuck latch.
5270 DEF SUB playKey
5280   IF Muted OR (CpcFxPri>3 AND TIME<CpcFxUntil) THEN EXIT SUB
5290   CpcFxPri=3 : CpcFxUntil=TIME+0.54
5300   SOUND 130,53,9,0,11 : SOUND 2,45,9,0,11 : SOUND 2,36,36,0,11
5310 SUBEND
5320 ' Heavy gate: descending rumble, noisy wood friction and one bright creak.
5330 DEF SUB playGate
5340   IF Muted OR (CpcFxPri>4 AND TIME<CpcFxUntil) THEN EXIT SUB
5350   CpcFxPri=4 : CpcFxUntil=TIME+0.78
5360   SOUND 130,844,78,0,12,3,20
5370 SUBEND
5380 ' Victory uses all three voices: melody, low D and delayed A bass.
5390 GOSUB 5210 : IF Muted THEN RETURN
5400 DI : CpcCelebrate=1 : CpcFIdx=0 : CpcFxPri=5 : CpcFxUntil=TIME+1.75
5410 GOSUB 5750 : SOUND 42,426,152,0,15
5420 SOUND 28,0,60,0 : SOUND 4,284,88,0,15 : EI : RETURN
5430 ' Two latch attempts; repeat is limited to once every half second.
5440 DEF SUB playLatch
5450   IF Muted OR T<LockUntil OR (CpcFxPri>2 AND TIME<CpcFxUntil) THEN EXIT SUB
5460   CpcFxPri=2 : CpcFxUntil=TIME+0.46 : LockUntil=T+0.5
5470   SOUND 130,240,9,0,13,4,7 : SOUND 2,0,8,0 : SOUND 2,260,29,0,13,4,9
5480 SUBEND
5490 ' Callbacks may interrupt FNCanOccupy, so they use no CALL and only private Cpc state.
5500 IF CpcPlay=0 OR Music=0 OR Muted OR Won THEN RETURN
5510 SOUND 1+CpcLSync(CpcLIdx),CpcLp(CpcLIdx),CpcLd(CpcLIdx),0,CpcLEnv(CpcLIdx),1
5520 CpcLIdx=(CpcLIdx+1) MOD CpcLn : ON SQ(1) GOSUB 5490
5530 RETURN
5540 ' Bass callback: each tone/rest pair is one quarter note.
5550 IF CpcPlay=0 OR Music=0 OR Muted OR Won THEN RETURN
5560 CpcBState=4 : CpcBEnv=0
5570 IF CpcBIdx MOD 2=0 THEN CpcBState=12:CpcBEnv=5
5580 SOUND CpcBState,CpcBp(CpcBIdx),CpcBd(CpcBIdx),0,CpcBEnv
5590 CpcBIdx=(CpcBIdx+1) MOD CpcBn : ON SQ(4) GOSUB 5540
5600 RETURN
5610 ' Percussion follows real TIME, not the game 's clamped motion clock T.
5620 DEF SUB updatePercussion
5630   LOCAL CpcDTick, CpcDrumType
5640   IF CpcPlay=0 OR Muted OR Won THEN EXIT SUB
5650   CpcDTick=INT((TIME-CpcStart)/0.15)
5660   IF CpcDTick<0 OR CpcDTick=CpcDrumIdx THEN EXIT SUB
5670   CpcDrumIdx=CpcDTick
5680   IF TIME<CpcFxUntil OR SQ(2)<>4 THEN EXIT SUB
5690   CpcDrumType=CpcDrum(CpcDTick MOD 16) : IF CpcDrumType=0 THEN EXIT SUB
5700   CpcFxPri=0
5710   IF CpcDrumType=1 THEN SOUND 2,690,7,0,6,2:EXIT SUB
5720   IF CpcDrumType=2 THEN SOUND 2,0,5,0,7,0,12:EXIT SUB
5730   SOUND 2,0,3,0,8,0,3
5740 SUBEND
5750 ' Finite fanfare: the final callback leaves no queue handler armed.
5760 IF CpcCelebrate=0 OR Muted THEN RETURN
5770 CpcFState=1 : CpcFEnv=14 : IF CpcFIdx=0 THEN CpcFState=49
5780 IF CpcFp(CpcFIdx)=0 THEN CpcFEnv=0
5790 SOUND CpcFState,CpcFp(CpcFIdx),CpcFd(CpcFIdx),0,CpcFEnv
5800 CpcFIdx=CpcFIdx+1
5810 IF CpcFIdx<8 THEN ON SQ(1) GOSUB 5750 ELSE CpcCelebrate=0
5820 RETURN
5830 ' DATA: lead period/duration/ENV/sync; bass period/duration; drum pattern.
5840 DATA 213,30,2,32,179,30,2,0,142,45,3,32,159,15,1,0
5850 DATA 190,30,2,32,213,30,2,0,179,15,1,32,159,15,1,0
5860 DATA 190,30,2,0,239,30,2,32,190,30,2,0,159,45,3,32
5870 DATA 179,15,1,0,213,30,2,32,239,30,2,0,190,30,2,32
5880 DATA 159,30,2,0,319,30,2,32,253,30,2,0,213,45,3,32
5890 DATA 239,15,1,0,284,30,2,32,319,30,2,0,253,15,1,32
5900 DATA 239,15,1,0,213,30,2,0,213,30,2,32,179,30,2,0
5910 DATA 190,60,4,32,239,30,2,32,284,30,2,0,213,30,2,32
5920 DATA 0,30,0,0
5930 DATA 426,41,0,19,568,41,0,19
5940 DATA 426,41,0,19,568,41,0,19
5950 DATA 478,41,0,19,638,41,0,19
5960 DATA 478,41,0,19,638,41,0,19
5970 DATA 638,41,0,19,426,41,0,19
5980 DATA 638,41,0,19,426,41,0,19
5990 DATA 426,41,0,19,568,41,0,19
6000 DATA 426,41,0,19,426,41,0,19
6010 DATA 1,0,3,0,2,0,3,0,1,0,3,0,2,0,3,0
6020 DATA 106,14,0,1,89,14,0,1,71,27,0,3,53,88,0,27
6030 ' Warm light is tied to fixed map coordinates, independent of the render grid.
6040 DEF SUB buildLighting(LightMap,VertexGlow,ObjectX,ObjectY)
6050   LOCAL Y,X,I,L,D2
6060   FOR Y=0 TO N-1 : FOR X=0 TO N-1
6070       L=0
6080       FOR I=0 TO Nt-1
6090         D2=(X+0.5-ObjectX(I))^2+(Y+0.5-ObjectY(I))^2 : L=MAX(L,1/(1+D2*1.6))
6100       NEXT I
6110       LightMap(Y,X)=L
6120   NEXT X : NEXT Y
6130   FOR Y=0 TO N : FOR X=0 TO N
6140       L=0 : FOR I=0 TO Nt-1 : L=MAX(L,1/(1+((X-ObjectX(I))^2+(Y-ObjectY(I))^2)*1.6)) : NEXT I
6150       VertexGlow(Y,X)=L
6160   NEXT X : NEXT Y
6170 SUBEND
6180 DEF SUB sortObjects(ObjectOrder,ObjectDistance,ObjectX,ObjectY)
6190   LOCAL I,J,Tmp
6200   FOR I=0 TO No-1 : ObjectOrder(I)=I : ObjectDistance(I)=(ObjectX(I)-Px)^2+(ObjectY(I)-Py)^2 : NEXT I
6210   FOR I=0 TO No-2 : FOR J=I+1 TO No-1
6220       IF ObjectDistance(ObjectOrder(I))<ObjectDistance(ObjectOrder(J)) THEN Tmp=ObjectOrder(I):ObjectOrder(I)=ObjectOrder(J):ObjectOrder(J)=Tmp
6230   NEXT J : NEXT I
6240 SUBEND
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
21000 ' MODE replaces only display state; maze, player, textures and audio survive.
21010 DEF SUB configureDisplay
21020   CanvasW=WIDTH : CanvasH=HEIGHT : UiScale=(CanvasH-1)/479
21025   Quality=2 : IF CanvasW=1280 THEN Quality=1
21030   Proj=(320/0.66)*(CanvasH/480) : Plane=CanvasW/(2*Proj) : ViewBottom=36 : ViewTop=CanvasH-40
21040   Cols=CanvasW/(2^(2-Quality)) : Cw=CanvasW/Cols : DIM D(CanvasW-1,CanvasH-1) : MAT D=0
21050   RAD : PAPER 0 : SMALLFONT TRANSPARENT : MOUSE OFF : SCALE 0,CanvasW-1,0,CanvasH-1
21055   IF SyncEnabled THEN SCREEN SYNC ON ELSE SCREEN SYNC OFF
21060   Last=TIME : TLast=Last : Count=0 : Dt=0
21070 SUBEND
21080 DEF SUB nextResolution
21090   IF ScreenMode=640 THEN ScreenMode=960 ELSE IF ScreenMode=960 THEN ScreenMode=1280 ELSE ScreenMode=640
21100   MODE ScreenMode,ScreenMode*9/16 : CALL configureDisplay
21110 SUBEND
21120 ' Explicit presentation keys remain correct after Alt+Enter changes state.
21130 DEF SUB displayInput
21140   IF K$="U" THEN SCREEN FULL ON
21150   IF K$="J" THEN SCREEN FULL OFF
21210 SUBEND
