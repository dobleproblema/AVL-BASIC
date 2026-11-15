10 REM Fraunhofer diffraction through a double slit, circle or equilateral triangle.
20 REM 1/2/3 choose aperture, Left/Right adjusts it, ESC exits.
30 MAT BASE 0 : RAD
40 GridSize=1024 : HalfGrid=GridSize/2 : Last=GridSize-1
45 Shape=1 : SlitSeparation=0.52 : Radius=0.3 : TriangleSide=0.76
50 Side=400 : LeftX=48 : RightX=512 : BottomY=64 : Span=16 : Gain=24000
60 DIM Aperture(Last,Last), Frequency(Last,0), PositionRow(0,Last)
70 DIM Shade(255), SampleIndex(Side-1)
80 ScreenBg=RGB(16,22,32) : PanelBg=RGB(10,16,24) : FrameColor=RGB(57,70,89)
90 TextColor=RGB(232,239,249) : Muted=RGB(164,183,206) : Gold=RGB(255,193,114)
95 GridColor=RGB(22,33,47)
100 FOR K=0 TO 255
110   Level=K/255
120   Shade(K)=RGB(INT(10+245*Level^1.5),INT(16+222*Level^3),INT(24+135*Level+20*SIN(PI*Level)))
130 NEXT K
135 GOSUB 2000
140 SCREEN : MODE 960,540 : PAPER ScreenBg
150 GOSUB 1000
160 FRAME 40 : Key$=INKEY$
170 IF Key$=CHR$(27) THEN SCREEN CLOSE:END
200 IF Key$="1" AND Shape<>1 THEN Shape=1:GOTO 150
210 IF Key$="2" AND Shape<>2 THEN Shape=2:GOTO 150
220 IF Key$="3" AND Shape<>3 THEN Shape=3:GOTO 150
240 StepSize=0
250 IF Key$=CHR$(29) THEN StepSize=0.04
260 IF Key$=CHR$(28) THEN StepSize=-0.04
270 IF StepSize=0 THEN GOTO 160
280 IF Shape=1 THEN SlitSeparation=MAX(0.16,MIN(0.88,SlitSeparation+StepSize))
290 IF Shape=2 THEN Radius=MAX(0.1,MIN(0.54,Radius+StepSize))
295 IF Shape=3 THEN TriangleSide=MAX(0.32,MIN(1.52,TriangleSide+StepSize))
300 GOTO 150
1000 REM Recompute the pattern when the aperture changes.
1060 GOSUB 2500
1070 StartTime=TIME
1080 REM Separable Fourier transform of the binary aperture.
1090 MAT HorizCos=CosBasis*Aperture
1100 MAT HorizSin=SinBasis*Aperture
1110 MAT RealPart=HorizCos*CosBasisT-HorizSin*SinBasisT
1120 MAT ImagPart=HorizCos*SinBasisT+HorizSin*CosBasisT
1125 REM Normalize intensity to the central peak before applying the display curve.
1130 OpenArea=SUM(Aperture) : Exposure=Gain/(OpenArea*OpenArea)
1140 MAT Pattern=MAP(LOG,1+Exposure*(RealPart@RealPart+ImagPart@ImagPart))
1150 MatTime=TIME-StartTime
1160 GOSUB 3000
1170 RETURN
2000 REM Precompute the Fourier bases and the full-field display sampling once.
2020 REM The phase grid is the outer product of angular frequency and position.
2030 FOR K=0 TO Last
2040   PositionRow(0,K)=(K-HalfGrid)/HalfGrid
2050   Frequency(K,0)=2*PI*Span*PositionRow(0,K)
2060 NEXT K
2070 MAT Phase=Frequency*PositionRow
2080 MAT CosBasis=MAP(COS,Phase)
2090 MAT SinBasis=MAP(SIN,Phase)
2100 MAT CosBasisT=TRN(CosBasis)
2110 MAT SinBasisT=TRN(SinBasis)
2120 REDIM Phase(0,0)
2130 FOR K=0 TO Side-1
2140   SampleIndex(K)=HalfGrid+ROUND((K-Side/2)*GridSize/Side)
2150 NEXT K
2160 ColorScale=255/LOG(1+Gain)
2170 RETURN
2500 REM Build the aperture on the source grid, with the origin at HalfGrid.
2510 MAT Aperture=0
2515 IF Shape=3 THEN GOTO 2700
2520 IF Shape=2 THEN GOTO 2610
2530 SlitOffset=ROUND(SlitSeparation*HalfGrid/2)
2540 SlitHalfWidth=ROUND(0.035*HalfGrid) : SlitHalfHeight=ROUND(0.24*HalfGrid)
2550 FOR J=HalfGrid-SlitHalfHeight TO HalfGrid+SlitHalfHeight
2560   FOR K=SlitOffset-SlitHalfWidth TO SlitOffset+SlitHalfWidth
2570     Aperture(HalfGrid+K,J)=1 : Aperture(HalfGrid-K,J)=1
2580   NEXT K
2590 NEXT J
2600 RETURN
2610 PixelRadius=ROUND(Radius*HalfGrid)
2620 FOR J=-PixelRadius TO PixelRadius
2630   RowHalf=INT(SQR(PixelRadius*PixelRadius-J*J))
2640   FOR K=HalfGrid-RowHalf TO HalfGrid+RowHalf
2650     Aperture(K,HalfGrid+J)=1
2660   NEXT K
2670 NEXT J
2680 RETURN
2700 REM An equilateral triangle centered at its centroid, filled row by row.
2710 RootThree=SQR(3) : Apothem=ROUND(TriangleSide*HalfGrid/(2*RootThree))
2720 FOR J=-Apothem TO 2*Apothem
2730   RowHalf=INT((2*Apothem-J)/RootThree)
2740   FOR K=HalfGrid-RowHalf TO HalfGrid+RowHalf
2750     Aperture(K,HalfGrid+J)=1
2760   NEXT K
2770 NEXT J
2780 RETURN
3000 REM Render both panels. Timing excludes presenting the frame and waiting for vsync.
3010 StartTime=TIME : CLG OFFSCREEN
3020 BIGFONT TRANSPARENT : INK TextColor : MOVE LeftX,521 : LABEL "Light diffraction bench"
3030 SMALLFONT TRANSPARENT : INK Muted
3040 MOVE RightX,521 : LABEL DEC$(6*GridSize^3/1000000000,"0.00")+" billion multiplications per calculation"
3050 MOVE LeftX,504 : LABEL "Fraunhofer diffraction / logarithmic intensity"
3060 INK Gold
3070 IF Shape=1 THEN Shape$="Double slit   separation "+DEC$(SlitSeparation,"0.00")
3080 IF Shape=2 THEN Shape$="Circular aperture   radius "+DEC$(Radius,"0.00")
3085 IF Shape=3 THEN Shape$="Equilateral triangle   side "+DEC$(TriangleSide,"0.00")
3090 MOVE LeftX,482 : LABEL Shape$
3100 Grid$=DEC$(GridSize,"0")
3110 MOVE RightX,482 : LABEL "Far field   "+Grid$+" x "+Grid$
3120 FRECTANGLE LeftX,BottomY,LeftX+Side-1,BottomY+Side-1,PanelBg
3130 FOR Mark=Side/8 TO Side-1 STEP Side/8
3140   MOVE LeftX+Mark,BottomY : DRAW LeftX+Mark,BottomY+Side-1,GridColor
3150   MOVE LeftX,BottomY+Mark : DRAW LeftX+Side-1,BottomY+Mark,GridColor
3160 NEXT Mark
3170 RECTANGLE LeftX-1,BottomY-1,LeftX+Side,BottomY+Side,FrameColor
3180 RECTANGLE RightX-1,BottomY-1,RightX+Side,BottomY+Side,FrameColor
3190 FOR V=0 TO Side-1
3200   J=SampleIndex(V) : ScreenY=BottomY+V
3210   FOR U=0 TO Side-1
3220     K=SampleIndex(U)
3230     IF Aperture(K,J)<>0 THEN PLOT LeftX+U,ScreenY,Gold
3240     PaletteIndex=INT(Pattern(K,J)*ColorScale+0.5)
3250     PLOT RightX+U,ScreenY,Shade(MIN(255,PaletteIndex))
3260   NEXT U
3270 NEXT V
3280 INK Muted : MOVE LeftX,21 : LABEL "1 / 2 / 3 aperture   Left / Right adjust"
3290 MOVE RightX,21 : LABEL "ESC exit"
3300 DrawTime=TIME-StartTime
3310 INK TextColor : MOVE LeftX,44 : LABEL "MAT "+DEC$(MatTime,"0.000")+" s"
3320 MOVE RightX,44 : LABEL "Draw "+DEC$(DrawTime,"0.000")+" s"
3330 FRAME
3340 RETURN
