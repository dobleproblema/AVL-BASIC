10 ' A first-person maze with textured walls and chiptune music.
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
160 LOCATE 3,2 : GPRINT "AVL DUNGEON CPC - CARVING STONE AND LIGHT..." : FRAME
170 T=0 : CALL loadMaze : CALL buildWallTextures : CALL buildObjectArt(Art,Pal) : CALL buildMaskRuns(Art,MaskRuns,MaskRunStart,MaskRunEnd,MaskRunColor) : CALL loadAudio : CALL resetPlay : GOSUB 5210 : GOSUB 5080
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
290     CALL resetPlay : GOSUB 5210 : GOSUB 5080 : Last=TIME
300   ELSE
310     GOSUB 5030
320     Cw=640/Cols
330     Turn=ABS(KEYDOWN(29))-ABS(KEYDOWN(28))
340     Walk=MAX(ABS(KEYDOWN(87)),ABS(KEYDOWN(30)))-MAX(ABS(KEYDOWN(83)),ABS(KEYDOWN(31)))
350     Strafe=ABS(KEYDOWN(68))-ABS(KEYDOWN(65)) : Running=ABS(KEYDOWN(32))
360     IF Won THEN Turn=0:Walk=0:Strafe=0
370     Ang=Ang+Turn*Dt*1.9 : Ang=Ang-2*PI*INT(Ang/(2*PI))
380     Dx=COS(Ang) : Dy=SIN(Ang) : Plx=-Dy*Plane : Ply=Dx*Plane
390     CALL movePlayer : CALL tryGate
400     IF VictoryPending THEN GOSUB 5380:VictoryPending=0
410     Hor=240+SIN(Pace)*2 : Tr=TIME : CLG OFFSCREEN
420     CALL drawPlanes : CALL drawWalls : CALL drawObjects : CALL drawLantern
430     IF MapOn THEN CALL drawMinimap
440     RenderMs=0.85*RenderMs+150*(TIME-Tr)
450     CALL drawHud
460     IF Cap THEN FRAME 60 ELSE FRAME
470     Count=Count+1 : Frames=Frames+1
480     IF TIME-TLast>=0.5 THEN Fps=Count/(TIME-TLast):Count=0:TLast=TIME
490   ENDIF
500 WEND
510 GOSUB 5210 : SCREEN CLOSE
520 END
530 ' Circle movement, normalized diagonals, separate-axis sliding
540 DEF SUB movePlayer
550   LOCAL Speed, Mx, My, Steps, Mstep, OldX, OldY, Nx
560   LOCAL Ny
570   Speed=2.2+Running*1.3 : Mx=(Dx*Walk-Dy*Strafe)*Speed*Dt : My=(Dy*Walk+Dx*Strafe)*Speed*Dt
580   IF Walk<>0 AND Strafe<>0 THEN Mx=Mx*0.70710678118655:My=My*0.70710678118655
590   Steps=MAX(1,INT(MAX(ABS(Mx),ABS(My))/0.08)+1) : Mx=Mx/Steps : My=My/Steps
600   OldX=Px : OldY=Py
610   FOR Mstep=1 TO Steps
620     Nx=Px+Mx : IF FNCanOccupy(W,Nx,Py) THEN Px=Nx
630     Ny=Py+My : IF FNCanOccupy(W,Px,Ny) THEN Py=Ny
640   NEXT Mstep
650   Moved=SQR((Px-OldX)^2+(Py-OldY)^2) : Pace=Pace+Moved*7
660   CALL updateFootsteps
670   Seen(INT(Py),INT(Px))=1
680   IF Have=0 AND (Px-Ox(12))^2+(Py-Oy(12))^2<0.3 THEN
690     Have=1:Msg$="GOLD KEY FOUND - FIND THE LOCKED GATE":MsgT=T+4:CALL playKey
700   ENDIF
710   IF Won=0 AND Opened AND (Px-Ox(13))^2+(Py-Oy(13))^2<0.28 THEN
720     Won=1:Msg$="YOU ESCAPED AVL DUNGEON!  PRESS R TO EXPLORE AGAIN":MsgT=T+99999:VictoryPending=1
730   ENDIF
740 SUBEND
750 ' Exact circle-to-solid-tile test; whole World array is passed by reference.
760 DEF FNCanOccupy(World,PosX,PosY)
770   LOCAL Cx,Cy,Qx,Qy
780   FNCanOccupy=0
790   IF PosX<Radius OR PosY<Radius OR PosX>N-Radius OR PosY>N-Radius THEN EXIT FN
800   FOR Cy=INT(PosY-Radius) TO INT(PosY+Radius)
810     FOR Cx=INT(PosX-Radius) TO INT(PosX+Radius)
820       IF World(Cy,Cx)<>0 THEN
830         Qx=MAX(Cx,MIN(Cx+1,PosX)) : Qy=MAX(Cy,MIN(Cy+1,PosY))
840         IF (PosX-Qx)^2+(PosY-Qy)^2<Radius*Radius THEN EXIT FN
850       ENDIF
860     NEXT Cx
870   NEXT Cy
880   FNCanOccupy=1
890 FNEND
900 ' The key is collected by proximity; E opens only the nearby gate
910 DEF SUB tryGate
920   LOCAL Dd, Facing
930   IF K$<>"E" OR Opened OR Won THEN EXIT SUB
940   Dd=(Px-14.5)^2+(Py-15.5)^2 : Facing=(14.5-Px)*Dx+(15.5-Py)*Dy
950   IF Dd>2.25 OR Facing<0.15 THEN Msg$="MOVE CLOSER AND FACE THE LOCKED GATE":MsgT=T+2:EXIT SUB
960   IF Have=0 THEN Msg$="THE GATE NEEDS THE GOLD KEY":MsgT=T+3:CALL playLatch:EXIT SUB
970   W(15,14)=0 : Opened=1 : Msg$="GATE UNLOCKED - REACH THE BLUE PORTAL" : MsgT=T+4 : CALL playGate
980 SUBEND
990 ' Reset play state; generated textures and display settings are retained
1000 DEF SUB resetPlay
1010   Px=2.5 : Py=2.5 : Ang=0 : Pace=0 : Have=0 : Opened=0 : Won=0
1020   MAT Seen=0 : W(15,14)=3 : RenderMs=0
1030   Msg$="FIND THE GOLD KEY, UNLOCK THE GATE, REACH THE PORTAL" : MsgT=T+4
1040   FootDist=0 : Foot=0 : LockUntil=0 : VictoryPending=0
1050 SUBEND
1060 ' Maze layout: nine rooms, loops, pillars and a sealed exit room
1070 DEF SUB loadMaze
1080   LOCAL Y, X, I, A$
1090   RESTORE 1270
1100   FOR Y=0 TO N-1
1110     READ Rows$(Y)
1120     FOR X=0 TO N-1
1130       A$=MID$(Rows$(Y),X+1,1) : W(Y,X)=0
1140       IF A$="#" THEN
1150         W(Y,X)=1 : IF (X*7+Y*3) MOD 11<3 THEN W(Y,X)=2
1160       ENDIF
1170       IF A$="D" THEN W(Y,X)=3
1180       IF A$="K" THEN Ox(12)=X+0.5:Oy(12)=Y+0.5
1190     NEXT X
1200   NEXT Y
1210   RESTORE 1350
1220   FOR I=0 TO Nt-1 : READ Ox(I),Oy(I),Wnx(I),Wny(I) : Kind(I)=0 : NEXT I
1230   Kind(12)=1 : Ox(13)=17.5 : Oy(13)=16.5 : Kind(13)=2
1240   CALL buildLighting(Light,Glow,Ox,Oy)
1250 SUBEND
1260 ' Move the single K in this map to reposition the gold key.
1270 DATA "###################","#.....##.....##...#","#.................#"
1280 DATA "#.....##.....##...#","#.....###.#####...#","##.######.######.##"
1290 DATA "##.###.....#####.##","#...##..#..##.....#","#..............#..#"
1300 DATA "#...##.....##..#.K#","#...#####.###..#..#","#...#####.###.....#"
1310 DATA "##.######.#...#####","##.#####....#######","#.....##....###...#"
1320 DATA "#.............D...#","#.....##....###..E#","#.....##....###...#"
1330 DATA "###################"
1340 ' Wall torch centres and outward normals; 0.04 in front of solid stone
1350 DATA 3.5,1.04,0,1,5.96,3.5,-1,0,10.5,1.04,0,1,11.5,3.96,0,-1
1360 DATA 17.96,2.5,-1,0,1.04,9.5,1,0,7.5,6.04,0,1,17.96,8.5,-1,0
1370 DATA 16.5,11.96,0,-1,3.5,17.96,0,-1,9.5,17.96,0,-1,16.5,14.04,0,1
1380 ' Stone/moss/iron-bound oak base texels; gentle detail limits shimmer
1390 DEF SUB buildWallTextures
1400   LOCAL M, Y, X, I, By, Bx, Grain, Var
1410   LOCAL R, G, B, L, Tile, Ax, Ay, Shade
1420   LOCAL C
1430   FOR M=0 TO 2
1440     FOR Y=0 TO 31
1450       FOR X=0 TO 31
1460         I=M*1024+Y*32+X : By=Y\8 : Bx=(X+(By MOD 2)*8) MOD 16
1470         Grain=3*SIN(X*0.8+Y*0.6)+2*COS(Y*0.9-X*0.4)
1480         Var=((X+(By MOD 2)*8)\16*7+By*11) MOD 13-6
1490         R=105+Var+Grain : G=116+Var+Grain : B=124+Var+Grain
1500         IF Bx<1 OR Y MOD 8<1 THEN R=39:G=44:B=51
1510         IF Bx=1 OR Y MOD 8=1 THEN R=R+15:G=G+15:B=B+15
1520         IF M=1 THEN R=R*0.83:G=G*0.94:B=B*0.82
1530         IF M=2 THEN
1540           R=97+10*SIN(X*0.8)+Grain : G=60+5*SIN(X*0.8)+Grain : B=32+Grain
1550           IF X MOD 8=0 THEN R=44:G=29:B=20
1560           IF Y=6 OR Y=7 OR Y=24 OR Y=25 THEN R=65:G=73:B=79
1570           IF (X=3 OR X=27) AND (Y=7 OR Y=24) THEN R=190:G=156:B=88
1580         ENDIF
1590         Br(I)=MAX(0,R) : Bg(I)=MAX(0,G) : Bb(I)=MAX(0,B)
1600       NEXT X
1610     NEXT Y
1620   NEXT M
1630   ' One immutable variable holds a 256 x 384 prelit texture atlas.
1640   ' Simple ATLAS$ enables the interpreter 's decoded texture cache.
1650   FOR M=0 TO 2
1660     FOR L=0 TO 31
1670       Tile=M*32+L : Ax=(Tile MOD 8)*32 : Ay=(Tile\8)*32 : Shade=0.11+L*0.89/31
1680       FOR Y=0 TO 31
1690         FOR X=0 TO 31
1700           I=M*1024+Y*32+X : C=RGB(INT(Br(I)*Shade),INT(Bg(I)*Shade),INT(Bb(I)*Shade))
1710           PLOT Ax+X,Ay+Y,C
1720         NEXT X
1730       NEXT Y
1740     NEXT L
1750   NEXT M
1760   Atlas$=SPRITE$(0,0,255,383)
1770   CALL buildPlaneTextures
1780 SUBEND
1790 ' Tiny pixel masks: wall-mounted torch, gold key, cyan escape portal
1800 DEF SUB buildObjectArt(Masks,Palette)
1810   LOCAL Y, X, D2
1820   MAT Masks=-1
1830   Palette(0)=RGB(42,44,48) : Palette(1)=RGB(108,94,65) : Palette(2)=RGB(188,118,44)
1840   Palette(3)=RGB(248,87,19) : Palette(4)=RGB(255,161,27) : Palette(5)=RGB(255,234,127)
1850   Palette(6)=RGB(236,179,57) : Palette(7)=RGB(255,231,123) : Palette(8)=RGB(20,99,141)
1860   Palette(9)=RGB(49,205,226) : Palette(10)=RGB(191,250,236)
1870   FOR Y=1 TO 10 : FOR X=5 TO 10 : Masks(0,Y,X)=0 : IF X=5 OR Y=10 THEN Masks(0,Y,X)=1
1880   NEXT X : NEXT Y
1890   FOR Y=7 TO 16 : FOR X=7 TO 8 : Masks(0,Y,X)=1
1900   NEXT X : NEXT Y
1910   FOR Y=14 TO 17 : FOR X=3 TO 12 : Masks(0,Y,X)=2 : NEXT X : NEXT Y
1920   FOR Y=17 TO 30
1930     FOR X=1 TO 14
1940       D2=((X-7.5)/6)^2+((Y-23)/8)^2
1950       IF D2<=1 THEN
1960         Masks(0,Y,X)=3 : IF D2<0.55 THEN Masks(0,Y,X)=4:IF D2<0.2 THEN Masks(0,Y,X)=5
1970       ENDIF
1980     NEXT X
1990   NEXT Y
2000   FOR Y=10 TO 25 : FOR X=0 TO 15
2010       D2=(X-4.5)^2+(Y-20)^2 : IF D2>=8 AND D2<21 THEN Masks(1,Y,X)=7
2020       IF (Y=19 OR Y=20) AND X>=8 THEN Masks(1,Y,X)=6
2030       IF (X=11 OR X=14) AND Y>=15 AND Y<=19 THEN Masks(1,Y,X)=7
2040   NEXT X : NEXT Y : CALL buildPortalArt(Masks)
2050 SUBEND
2060 ' One unlit texture per plane; TQUAD supplies the vertex lighting.
2070 DEF SUB buildPlaneTextures
2080   LOCAL Y, X, Grain, Luma
2090   FOR Y=0 TO 31 : FOR X=0 TO 31
2100       Grain=3*SIN(X*0.6+Y*0.9) : Luma=93+Grain+((X\16+Y\16) MOD 2)*8
2110       IF X MOD 16=0 OR Y MOD 16=0 THEN Luma=56
2120       PLOT X,Y,RGB(INT(Luma),INT(Luma*1.02),INT(Luma*1.03))
2130       Luma=65+Grain : IF (X+(Y\8 MOD 2)*8) MOD 16=0 OR Y MOD 8=0 THEN Luma=39
2140       PLOT X+40,Y,RGB(INT(Luma*0.75),INT(Luma*0.84),INT(Luma))
2150   NEXT X : NEXT Y
2160   Floor$=SPRITE$(0,0,31,31) : Ceil$=SPRITE$(40,0,71,31)
2170 SUBEND
2180 DEF SUB buildPortalArt(Masks)
2190   LOCAL Y, X, D2
2200   FOR Y=1 TO 30 : FOR X=0 TO 15
2210       D2=((X-7.5)/7)^2+((Y-16)/14)^2
2220       IF D2>0.75 AND D2<=1 THEN Masks(2,Y,X)=9
2230       IF D2>0.86 AND D2<0.95 THEN Masks(2,Y,X)=10
2240       IF D2<=0.75 THEN Masks(2,Y,X)=8
2250       IF Y<12 AND D2<0.75 AND Y MOD 3=0 THEN Masks(2,Y,X)=10
2260   NEXT X : NEXT Y
2270 SUBEND
2280 ' Fixed world tiles keep texture and torch light in the same place.
2281 ' Solid cells need no planes; wall/floor rounding can change a few edge pixels.
2290 DEF SUB drawPlanes
2300   LOCAL Y, X, Cx, Cz, Near, Pad, Edge, Flicker
2310   LOCAL Half
2320   FRECTANGLE 0,40,639,439,RGB(9,13,19) : MAT D=0
2330   Near=0.1 : Pad=SQR(0.5) : Edge=Pad*SQR(1+Plane^2) : Flicker=1+0.045*SIN(T*10)+0.025*SIN(T*17)
2340   FOR Y=0 TO N : FOR X=0 TO N
2350       CamX(Y,X)=-(X-Px)*Dy+(Y-Py)*Dx : CamZ(Y,X)=(X-Px)*Dx+(Y-Py)*Dy
2360   NEXT X : NEXT Y
2370   FOR Y=0 TO N-1 : FOR X=0 TO N-1
2380       Cx=CamX(Y,X)+(Dx-Dy)*0.5 : Cz=CamZ(Y,X)+(Dx+Dy)*0.5
2390       IF W(Y,X)=0 AND Cz+Pad>=Near AND ABS(Cx)<=Plane*Cz+Edge THEN
2400         Ax(0)=CamX(Y,X) : Az(0)=CamZ(Y,X) : Au(0)=X*32 : Av(0)=Y*32 : Al(0)=Glow(Y,X)
2410         Ax(1)=CamX(Y,X+1) : Az(1)=CamZ(Y,X+1) : Au(1)=(X+1)*32 : Av(1)=Y*32 : Al(1)=Glow(Y,X+1)
2420         Ax(2)=CamX(Y+1,X+1) : Az(2)=CamZ(Y+1,X+1) : Au(2)=(X+1)*32 : Av(2)=(Y+1)*32 : Al(2)=Glow(Y+1,X+1)
2430         Ax(3)=CamX(Y+1,X) : Az(3)=CamZ(Y+1,X) : Au(3)=X*32 : Av(3)=(Y+1)*32 : Al(3)=Glow(Y+1,X)
2440         IF MAX(Az(0),Az(1),Az(2),Az(3))>=Near THEN
2450           IF MIN(Az(0),Az(1),Az(2),Az(3))>=Near THEN
2460             Bn=4 : MAT Bx=Ax : MAT Bz=Az : MAT Bu=Au : MAT Bv=Av : MAT Bl=Al
2470             CALL drawClippedPlane(Flicker)
2480           ELSE
2490             ' Clip the original triangles separately to preserve their lighting.
2500             FOR Half=0 TO 1 : CALL clipPlaneTriangle(Half,Near,Flicker) : NEXT Half
2510           ENDIF
2520         ENDIF
2530       ENDIF
2540   NEXT X : NEXT Y
2550 SUBEND
2560 ' Clip triangle 0,1,2 or 0,2,3 against the near plane.
2570 DEF SUB clipPlaneTriangle(Half,Near,Flicker)
2580   LOCAL K0, K1, K, Mix
2590   Bn=0 : K0=Half+2
2600   FOR K=0 TO 2
2610     K1=0 : IF K>0 THEN K1=Half+K
2620     IF (Az(K0)>=Near)<>(Az(K1)>=Near) THEN
2630       Mix=(Near-Az(K0))/(Az(K1)-Az(K0))
2640       Bx(Bn)=Ax(K0)+(Ax(K1)-Ax(K0))*Mix : Bz(Bn)=Near : Bu(Bn)=Au(K0)+(Au(K1)-Au(K0))*Mix : Bv(Bn)=Av(K0)+(Av(K1)-Av(K0))*Mix : Bl(Bn)=Al(K0)+(Al(K1)-Al(K0))*Mix : Bn=Bn+1
2650     ENDIF
2660     IF Az(K1)>=Near THEN
2670       Bx(Bn)=Ax(K1) : Bz(Bn)=Az(K1) : Bu(Bn)=Au(K1) : Bv(Bn)=Av(K1) : Bl(Bn)=Al(K1) : Bn=Bn+1
2680     ENDIF
2690     K0=K1 : NEXT K
2700   CALL drawClippedPlane(Flicker)
2710 SUBEND
2720 DEF SUB drawClippedPlane(Flicker)
2730   LOCAL J, Gl
2740   IF Bn<3 THEN EXIT SUB
2750   FOR J=0 TO Bn-1
2760     Fq(J)=1/Bz(J) : Fx(J)=320+Proj*Bx(J)*Fq(J) : Fy(J)=Hor-0.5*Proj*Fq(J) : Gl=Bl(J)*Flicker
2770     Fr(J)=255*(0.12+1.8*Gl) : Fg(J)=255*(0.14+1.2*Gl) : Fb(J)=255*(0.18+0.55*Gl)
2780   NEXT J
2790   IF Bn=4 THEN
2800     TQUAD Floor$,D,Fx(0),Fy(0),Fq(0),Bu(0),Bv(0),Fr(0),Fg(0),Fb(0),Fx(1),Fy(1),Fq(1),Bu(1),Bv(1),Fr(1),Fg(1),Fb(1),Fx(2),Fy(2),Fq(2),Bu(2),Bv(2),Fr(2),Fg(2),Fb(2),Fx(3),Fy(3),Fq(3),Bu(3),Bv(3),Fr(3),Fg(3),Fb(3)
2810     TQUAD Ceil$,D,Fx(0),2*Hor-Fy(0),Fq(0),Bu(0),Bv(0),Fr(0),Fg(0),Fb(0),Fx(1),2*Hor-Fy(1),Fq(1),Bu(1),Bv(1),Fr(1),Fg(1),Fb(1),Fx(2),2*Hor-Fy(2),Fq(2),Bu(2),Bv(2),Fr(2),Fg(2),Fb(2),Fx(3),2*Hor-Fy(3),Fq(3),Bu(3),Bv(3),Fr(3),Fg(3),Fb(3)
2820   ELSE
2830     TTRIANGLE Floor$,D,Fx(0),Fy(0),Fq(0),Bu(0),Bv(0),Fr(0),Fg(0),Fb(0),Fx(1),Fy(1),Fq(1),Bu(1),Bv(1),Fr(1),Fg(1),Fb(1),Fx(2),Fy(2),Fq(2),Bu(2),Bv(2),Fr(2),Fg(2),Fb(2)
2840     TTRIANGLE Ceil$,D,Fx(0),2*Hor-Fy(0),Fq(0),Bu(0),Bv(0),Fr(0),Fg(0),Fb(0),Fx(1),2*Hor-Fy(1),Fq(1),Bu(1),Bv(1),Fr(1),Fg(1),Fb(1),Fx(2),2*Hor-Fy(2),Fq(2),Bu(2),Bv(2),Fr(2),Fg(2),Fb(2)
2850   ENDIF
2860 SUBEND
2870 ' DDA rays use camera-plane distance: walls and sprites share one depth
2880 DEF SUB drawWalls
2890   LOCAL C, Camera, Rx, Ry, Gx, Gy, DelX, DelY
2900   LOCAL Sx, Sy, Sdx, Sdy, Dstep, Dist, Matl
2910   LOCAL Side, WallX, TexX, Gl, Shade, Sh, Tile, U
2920   LOCAL V, Lh, WLow, WHigh, Yb, Yt, Vb, Vt
2925   ' The maze keeps a solid border; every ray hits before N*2 steps.
2930   FOR C=0 TO Cols-1
2940     Camera=2*(C+0.5)/Cols-1 : Rx=Dx+Plx*Camera : Ry=Dy+Ply*Camera
2950     Gx=INT(Px) : Gy=INT(Py) : DelX=1E+30 : DelY=1E+30
2960     IF ABS(Rx)>1E-12 THEN DelX=ABS(1/Rx)
2970     IF ABS(Ry)>1E-12 THEN DelY=ABS(1/Ry)
2980     Sx=1 : Sy=1 : Sdx=(Gx+1-Px)*DelX : Sdy=(Gy+1-Py)*DelY
2990     IF Rx<0 THEN Sx=-1:Sdx=(Px-Gx)*DelX
3000     IF Ry<0 THEN Sy=-1:Sdy=(Py-Gy)*DelY
3020     FOR Dstep=1 TO N*2
3030       IF Sdx<Sdy THEN Sdx=Sdx+DelX:Gx=Gx+Sx:Side=0 ELSE Sdy=Sdy+DelY:Gy=Gy+Sy:Side=1
3050       Seen(Gy,Gx)=1
3060       Matl=W(Gy,Gx) : IF Matl>0 THEN EXIT FOR
3070     NEXT Dstep
3090     Dist=Sdx-DelX : IF Side=1 THEN Dist=Sdy-DelY
3100     Dist=MAX(0.001,Dist)
3140     ZBuf(C)=Dist : WallX=Py+Dist*Ry : IF Side=1 THEN WallX=Px+Dist*Rx
3150     WallX=WallX-INT(WallX) : TexX=INT(WallX*32)
3160     IF (Side=0 AND Rx>0) OR (Side=1 AND Ry<0) THEN TexX=31-TexX
3170     Gl=Light(Gy,Gx)
3180     Shade=(0.1+0.82/(1+Dist*0.2)+Gl*0.13) : IF Side=1 THEN Shade=Shade*0.82
3190     Sh=MAX(0,MIN(31,INT(Shade*31))) : Tile=(Matl-1)*32+Sh
3200     U=(Tile MOD 8)*32+TexX+0.5 : V=(Tile\8)*32 : Lh=Proj/Dist
3210     WLow=Hor-Lh*0.5 : WHigh=Hor+Lh*0.5 : Yb=MAX(40,WLow) : Yt=MIN(440,WHigh)
3220     Vb=V+(Yb-WLow)*32/Lh : Vt=V+(Yt-WLow)*32/Lh
3230     TRECTANGLE Atlas$,C*Cw,Yb,(C+1)*Cw,Yt,U,Vb,U,Vt
3240   NEXT C
3250 SUBEND
3260 ' Depth-sort objects; torches use wall planes, key and portal billboards
3270 DEF SUB drawObjects
3280   LOCAL I, Obj, Sk, Ddy, Ddx, Depth, Across
3290   CALL sortObjects(Order,Od,Ox,Oy)
3300   FOR Obj=0 TO No-1
3310     I=Order(Obj) : Sk=Kind(I)
3320     IF Sk<>1 OR Have=0 THEN
3330       Ddy=Oy(I)-Py : Ddx=Ox(I)-Px : Depth=Ddx*Dx+Ddy*Dy : Across=-Ddx*Dy+Ddy*Dx
3340       IF Sk=0 THEN CALL drawWallTorch(I,Ddx,Ddy,Depth,Across) ELSE CALL drawBillboard(Sk,Depth,Across)
3350     ENDIF
3360   NEXT Obj
3370 SUBEND
3380 ' Key and portal billboards; reject invisible objects before scanning their masks.
3389 ' Precalculated immutable mask runs preserve color order and transparent gaps.
3390 DEF SUB drawBillboard(Sk,Depth,Across)
3400   LOCAL Center,Sw,Sh,Z0,SLeft,Bottom,XLo,XHi, RunIndex
3410   LOCAL Sc,Tx,LastCol,StartY,Ty,Yb,Yt
3420   IF Depth<0.12 OR Depth>Far THEN EXIT SUB
3430   Center=320+Proj*Across/Depth
3440   IF Sk=1 THEN Sw=Proj*0.46/Depth:Sh=Proj*0.46/Depth:Z0=0.25+0.035*SIN(T*3) ELSE Sw=Proj*0.75/Depth:Sh=Proj*0.91/Depth:Z0=0.02
3450   SLeft=Center-Sw*0.5 : Bottom=Hor+(Z0-0.5)*Proj/Depth
3460   IF SLeft>639 OR SLeft+Sw<0 THEN EXIT SUB
3470   XLo=MAX(0,INT(SLeft/Cw)) : XHi=MIN(Cols-1,INT((SLeft+Sw)/Cw))
3480   FOR Sc=XLo TO XHi
3490     Tx=INT(((Sc+0.5)*Cw-SLeft)*16/Sw)
3500     IF Depth<ZBuf(Sc) AND Tx>=0 AND Tx<=15 THEN
3520       FOR RunIndex=0 TO MaskRuns(Sk,Tx)-1
3530         StartY=MaskRunStart(Sk,Tx,RunIndex) : Ty=MaskRunEnd(Sk,Tx,RunIndex) : LastCol=MaskRunColor(Sk,Tx,RunIndex)
3540         Yb=Bottom+StartY*Sh/32 : Yt=Bottom+Ty*Sh/32
3550         Yb=MAX(40,INT(Yb)) : Yt=MIN(439,INT(Yt))
3560         IF Yt>=Yb THEN FRECTANGLE Sc*Cw,Yb,(Sc+1)*Cw-1,Yt,Pal(LastCol)
3570       NEXT RunIndex
3630     ENDIF
3640   NEXT Sc
3650 SUBEND
3660 ' Wall-aligned torch planes, elevated above the floor on iron brackets
3669 ' Precalculated immutable mask runs preserve color order and transparent gaps.
3670 DEF SUB drawWallTorch(I,Ddx,Ddy,Depth,Across)
3680   LOCAL Pn, Vx, Vy, Tpd, Tpa, Td1, Tc1, Td2, RunIndex
3690   LOCAL Tc2, X1, X2, XLo, XHi, Flicker, Sc, Camera
3700   LOCAL Rx, Ry, Den, Td, Tx, Sh, Bottom, LastCol
3710   LOCAL StartY, Ty, Yb, Yt
3720   Pn=Ddx*Wnx(I)+Ddy*Wny(I) : IF Pn>=-0.015 THEN EXIT SUB
3730   Vx=-Wny(I) : Vy=Wnx(I) : Tpd=Vx*Dx+Vy*Dy : Tpa=-Vx*Dy+Vy*Dx
3740   Td1=Depth-0.12*Tpd : Tc1=Across-0.12*Tpa : Td2=Depth+0.12*Tpd : Tc2=Across+0.12*Tpa
3750   IF Td1<=0.08 AND Td2<=0.08 THEN EXIT SUB
3760   IF Td1<0.08 THEN Tc1=Tc1+(Tc2-Tc1)*(0.08-Td1)/(Td2-Td1):Td1=0.08
3770   IF Td2<0.08 THEN Tc2=Tc2+(Tc1-Tc2)*(0.08-Td2)/(Td1-Td2):Td2=0.08
3780   X1=320+Proj*Tc1/Td1 : X2=320+Proj*Tc2/Td2
3790   XLo=MAX(0,INT(MIN(X1,X2)/Cw)) : XHi=MIN(Cols-1,INT(MAX(X1,X2)/Cw))
3800   IF XLo>XHi THEN EXIT SUB
3810   Flicker=1+0.025*SIN(T*10+I*2.7)+0.018*SIN(T*17+I)
3820   FOR Sc=XLo TO XHi
3830     Camera=2*(Sc+0.5)/Cols-1 : Rx=Dx+Plx*Camera : Ry=Dy+Ply*Camera : Den=Rx*Wnx(I)+Ry*Wny(I)
3840     IF ABS(Den)>=1E-9 THEN
3850       Td=Pn/Den
3860       IF Td>=0.08 AND Td<ZBuf(Sc) AND Td<=Far THEN
3870         Tx=INT((0.5+((Px+Rx*Td-Ox(I))*Vx+(Py+Ry*Td-Oy(I))*Vy)/0.24)*16)
3880         IF Tx>=0 AND Tx<=15 THEN
3890           Sh=Proj*0.52/Td : Bottom=Hor-0.1*Proj/Td
3900           FOR RunIndex=0 TO MaskRuns(0,Tx)-1
3910             StartY=MaskRunStart(0,Tx,RunIndex) : Ty=MaskRunEnd(0,Tx,RunIndex) : LastCol=MaskRunColor(0,Tx,RunIndex)
3920             Yb=Bottom+StartY*Sh/32 : Yt=Bottom+Ty*Sh/32
3930             IF StartY>=17 THEN Yb=Bottom+(Yb-Bottom)*Flicker:Yt=Bottom+(Yt-Bottom)*Flicker
3940             Yb=MAX(40,INT(Yb)) : Yt=MIN(439,INT(Yt))
3950             IF Yt>=Yb THEN FRECTANGLE Sc*Cw,Yb,(Sc+1)*Cw-1,Yt,Pal(LastCol)
3960           NEXT RunIndex
4020         ENDIF
4030       ENDIF
4040     ENDIF
4050   NEXT Sc
4060 SUBEND
4070 ' Explored minimap; unseen rooms stay hidden until rays visit them
4080 DEF SUB drawMinimap
4090   LOCAL Y, X, Mc, Ax, Ay
4100   FRECTANGLE 508,310,635,437,RGB(7,14,20)
4110   RECTANGLE 508,310,635,437,RGB(105,127,120)
4120   FOR Y=0 TO N-1 : FOR X=0 TO N-1
4130       IF Seen(Y,X) THEN
4140         Mc=RGB(46,56,57) : IF W(Y,X)>0 THEN Mc=RGB(116,126,124)
4150         IF W(Y,X)=3 THEN Mc=RGB(155,101,42)
4160         FRECTANGLE 515+X*6,431-Y*6,519+X*6,427-Y*6,Mc
4170       ENDIF
4180   NEXT X : NEXT Y
4190   IF Have=0 AND Seen(INT(Oy(12)),INT(Ox(12))) THEN FCIRCLE 515+Ox(12)*6,432-Oy(12)*6,2,Pal(7)
4200   IF Seen(16,17) THEN FCIRCLE 518+17*6,429-16*6,2,Pal(9)
4210   Ax=515+Px*6 : Ay=432-Py*6 : FCIRCLE Ax,Ay,2,RGB(244,225,155)
4220   DRAW Ax,Ay,Ax+Dx*6,Ay-Dy*6,RGB(244,225,155)
4230 SUBEND
4240 ' Opaque brass, alpha glass and flame glow; the handle stays open
4250 DEF SUB drawLantern
4260   LOCAL Lx, Ly, Fh
4270   Lx=548+SIN(Pace)*2 : Ly=42+COS(Pace)*3
4280   FRECTANGLE Lx+5,Ly+10,Lx+38,Ly+60,RGB(85,139,144) ALPHA 0.18
4290   CIRCLE Lx+22,Ly+81,11,RGB(152,112,51) : CIRCLE Lx+22,Ly+81,10,RGB(213,163,75)
4300   FRECTANGLE Lx,Ly,Lx+43,Ly+9,RGB(182,134,60) : FRECTANGLE Lx,Ly+60,Lx+43,Ly+68,RGB(182,134,60)
4310   FRECTANGLE Lx,Ly+10,Lx+4,Ly+59,RGB(182,134,60) : FRECTANGLE Lx+39,Ly+10,Lx+43,Ly+59,RGB(182,134,60)
4320   FCIRCLE Lx+22,Ly+27,10,RGB(255,161,46),1.7 ALPHA 0.2
4330   Fh=45+SIN(T*11)*1.3 : FTRIANGLE Lx+13,Ly+23,Lx+31,Ly+23,Lx+22+SIN(T*7)*2,Ly+Fh,RGB(248,145,33) ALPHA 0.9
4340   FTRIANGLE Lx+18,Ly+23,Lx+26,Ly+23,Lx+22,Ly+36,RGB(255,234,150)
4350   FRECTANGLE Lx+20,Ly+9,Lx+24,Ly+18,RGB(198,158,89)
4360   FRECTANGLE Lx+3,Ly+4,Lx+40,Ly+8,RGB(240,190,103)
4370   FRECTANGLE Lx-4,Ly+61,Lx+47,Ly+67,RGB(213,163,75)
4380   FRECTANGLE Lx+18,Ly+10,Lx+20,Ly+59,RGB(181,138,61)
4390 SUBEND
4400 ' HUD and the win overlay are deliberately drawn after the world
4410 DEF SUB drawHud
4420   LOCAL Ks$, Fs$, As$
4430   FRECTANGLE 0,0,639,39,RGB(10,17,23) : FRECTANGLE 0,440,639,479,RGB(10,17,23)
4440   DRAW 0,439,639,439,RGB(124,105,63) : DRAW 0,40,639,40,RGB(124,105,63)
4450   IF Hud=0 AND Won=0 THEN EXIT SUB
4460   SMALLFONT TRANSPARENT : INK RGB(233,214,173)
4470   Ks$="--" : IF Have THEN Ks$="GOLD"
4480   Fs$="FREE" : IF Cap THEN Fs$="CAP60"
4490   As$="OFF" : IF Music AND Won=0 THEN As$="ON"
4500   IF Muted THEN As$="MUTE"
4510   LOCATE 2,0 : GPRINT "DUNGEON CPC  ";Cols;" COLS  KEY:";Ks$;"  ";USING "###.0";Fps;" FPS ";Fs$;"  AUDIO:";As$
4520   INK RGB(143,174,182) : LOCATE 2,1
4530   IF T<MsgT THEN GPRINT Msg$ ELSE GPRINT "RENDER ";USING "##.0";RenderMs;" ms  |  FIND THE KEY - OPEN THE GATE - ESCAPE"
4540   LOCATE 2,28 : GPRINT "W/S OR UP/DOWN WALK  LEFT/RIGHT TURN  A/D STRAFE  SPACE RUN  E GATE"
4550   LOCATE 2,29 : GPRINT "M MAP  Q QUALITY  F FPS  R RESTART  H HUD  B MUSIC  N MUTE  ESC EXIT"
4560   IF Won=0 THEN EXIT SUB
4570   FRECTANGLE 115,183,525,292,RGB(10,22,29) : RECTANGLE 115,183,525,292,Pal(9)
4580   BIGFONT TRANSPARENT : INK Pal(10) : LOCATE 14,13 : GPRINT "YOU ESCAPED!"
4590   SMALLFONT TRANSPARENT : INK RGB(232,208,147) : LOCATE 27,17 : GPRINT "R - RETURN TO THE DUNGEON"
4600 SUBEND
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
