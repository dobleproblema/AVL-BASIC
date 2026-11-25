10 REM Explore the two embedded bitmap fonts. All positions in the chart are in Unicode order.
20 MODE 960 : SCREEN : PAPER &H17212A : CLG OFFSCREEN : SMALLFONT TRANSPARENT
30 Accent=&H8BDDC9 : TextInk=&HF8EDD8 : Muted=&HB7C7D3 : Rule=&H3A4A54 : Dark=&H17212A
40 Glyphs$="" : FOR Code=32 TO 126 : Glyphs$=Glyphs$+CHR$(Code) : NEXT Code
50 FOR Code=160 TO 255 : Glyphs$=Glyphs$+CHR$(Code) : NEXT Code
80 Glyphs$=Glyphs$+"ĂăĄąĆćČčĎďĐđĘęĚěĹĺĽľŁłŃńŇňŐőŒœŔŕŘřŚśŞşŠšŢţŤťŮůŰűŸŹźŻżŽž"
90 Glyphs$=Glyphs$+"ȘșȚțˇ˘˙˛˝ΓΔΘΛΠΣΦΩαβγδεζηθκλμνξπρστφχψω"
92 Glyphs$=Glyphs$+"ẞ”„€←↑→↓↔↕⇒⇔∀∂∃∇∈∉∏∑∓√∝∞∧∨∩∪∫≃≈≠≡≤≥□"
100 MAT BASE 0 : READ SampleCount : DIM Sample$(SampleCount-1),Words$(SampleCount-1)
110 FOR I=0 TO SampleCount-1 : READ Sample$(I),Words$(I) : NEXT I
120 CellH=24 : ChartX=16 : ChartTop=78 : RowsShown=MIN(12,SampleCount)
125 ChartRows=16 : GlyphCount=LEN(Glyphs$) : SmallGrid=0
130 PageNo=1 : Pick=INSTR(Glyphs$,"Ł") : FirstRow=0 : Redraw=1 : GOSUB GridLayout
140 ON MOUSE LEFTDOWN GOSUB Clicked : ON MOUSE WHEEL GOSUB Scrolled
150 MainLoop:
160 IF Redraw THEN Redraw=0 : GOSUB DrawPage
170 FRAME 50 : K$=UPPER$(INKEY$)
180 IF K$=CHR$(27) THEN SCREEN CLOSE : END
190 IF K$="1" THEN PageNo=1 : Redraw=1
200 IF K$="2" THEN PageNo=2 : Redraw=1
210 IF K$=" " THEN PageNo=3-PageNo : Redraw=1
220 IF ClickPending THEN ClickPending=0 : GOSUB HandleClick
230 IF PageNo=1 THEN GOSUB ChartKeys ELSE GOSUB ExampleKeys
240 GOTO MainLoop

300 DrawPage:
310 CLG OFFSCREEN : BIGFONT : MOVE 16,HEIGHT-19
320 IF PageNo=1 THEN LABEL "Bitmap font atlas",TextInk : GOSUB Characters ELSE LABEL "Languages and mathematics",TextInk : GOSUB Examples
330 SMALLFONT : MOVE 16,44 : DRAW WIDTH-17,44,Rule
340 MOVE 16,30 : LABEL "1  Characters",IIF(PageNo=1,Accent,Muted)
350 MOVE 180,30 : LABEL "2  Examples",IIF(PageNo=2,Accent,Muted)
360 MOVE 372,30 : LABEL "Space  Switch page       Esc  Quit",Muted
370 RETURN

400 Characters:
405 FirstGlyph=FirstChartRow*Cols+1 : LastGlyph=MIN(GlyphCount,FirstGlyph+ChartRows*Cols-1)
410 SMALLFONT : MOVE ChartX,HEIGHT-53 : LABEL GridName$+"   "+DEC$(FirstGlyph,"0")+"-"+DEC$(LastGlyph,"0")+" of "+DEC$(GlyphCount,"0")+" in Unicode order",Accent
420 IF SmallGrid THEN SMALLFONT ELSE BIGFONT
430 FOR I=FirstGlyph TO LastGlyph
440   X=ChartX+((I-FirstGlyph) MOD Cols)*CellW : Y=HEIGHT-1-ChartTop-INT((I-FirstGlyph)/Cols)*CellH
450   IF I=Pick THEN FRECTANGLE X,Y+3,X+CellW-3,Y-18,Accent
460   MOVE X+GlyphInset,Y : LABEL Glyphs$[I],IIF(I=Pick,Dark,TextInk)
470 NEXT I
480 BIGFONT : MOVE 536,HEIGHT-53 : DRAW 536,80,Rule
490 Ch$=Glyphs$[Pick] : MOVE 561,HEIGHT-53 : LABEL "U+"+HEX$(ASC(Ch$),4),Accent
500 SMALLFONT : MOVE 561,HEIGHT-91 : LABEL "SMALLFONT  8×16",Muted
510 MOVE 718,HEIGHT-91 : LABEL "BIGFONT  16×16",Muted
520 FOR F=1 TO 2
530   IF F=1 THEN SMALLFONT : X=561 : W=8 ELSE BIGFONT : X=718 : W=16
540   MOVE X,HEIGHT-117 : LABEL Ch$,TextInk
550   FOR Py=0 TO 15 : FOR Px=0 TO W-1
560     PixelInk=Rule : IF TEST(X+Px,HEIGHT-117-Py)=TextInk THEN PixelInk=TextInk
570     LeftX=X+12*Px : TopY=HEIGHT-163-12*Py
580     FRECTANGLE LeftX,TopY,LeftX+10,TopY-10,PixelInk
590   NEXT Px : NEXT Py
600 NEXT F
610 SMALLFONT : MOVE 561,HEIGHT-386
620 IF Ch$=" " THEN LABEL "Space",Muted
630 IF Ch$=CHR$(160) THEN LABEL "Non-breaking space",Muted
640 IF Ch$=CHR$(173) THEN LABEL "Soft hyphen",Muted
650 IF Ch$="□" THEN LABEL "Replacement square",Muted
660 MOVE ChartX,69 : Hint$=IIF(SmallGrid,"F  Show BIGFONT","F  Show SMALLFONT")
662 Hint$=Hint$+"    Click or arrows  Select    Home/End  First/last"
664 IF SmallGrid=0 THEN Hint$=Hint$+"    Wheel or PgUp/PgDn  Scroll"
666 LABEL Hint$,Muted
670 RETURN

700 Examples:
710 Formula$="A = πr²   √9 = 3   ∫ 2x dx = x² + C   ½ + ¼ = ¾   π ≈ 3.14"
720 SMALLFONT : MOVE 16,HEIGHT-54 : LABEL Formula$,Muted
730 BIGFONT : MOVE 16,HEIGHT-78 : LABEL Formula$,TextInk
740 SMALLFONT : MOVE 16,HEIGHT-118 : LABEL "SAMPLE",Accent
750 MOVE 154,HEIGHT-118 : LABEL "SMALLFONT",Accent
760 MOVE 426,HEIGHT-118 : LABEL "BIGFONT",Accent
770 FOR J=0 TO RowsShown-1
780   I=FirstRow+J : Y=HEIGHT-149-27*J
790   SMALLFONT : MOVE 16,Y : LABEL Sample$(I),Muted
800   MOVE 154,Y : LABEL Words$(I),TextInk
810   BIGFONT : MOVE 426,Y : LABEL Words$(I),TextInk
820 NEXT J
830 SMALLFONT : Status$=DEC$(FirstRow+1,"0")+"-"+DEC$(FirstRow+RowsShown,"0")+" / "+DEC$(SampleCount,"0")
840 MOVE 16,68 : LABEL Status$,Accent
850 MOVE 154,68 : LABEL "Mouse wheel or Up/Down  Scroll     Home/End  First/last",Muted
860 RETURN

900 ChartKeys:
901 IF K$="F" THEN
902   IF SmallGrid=0 THEN BigTop=FirstChartRow
903   SmallGrid=1-SmallGrid : GOSUB GridLayout : Redraw=1 : RETURN
904 ENDIF
910 WholeRows=FIX(ChartWheel) : ChartWheel=ChartWheel-WholeRows
912 IF WholeRows=0 AND K$="" THEN RETURN
915 NewTop=MAX(0,MIN(LastTop,FirstChartRow-WholeRows))
920 PickRow=INT((Pick-1)/Cols) : PickCol=(Pick-1) MOD Cols
925 PickRow=MAX(NewTop,MIN(NewTop+ChartRows-1,PickRow)) : NewPick=MIN(GlyphCount,PickRow*Cols+PickCol+1)
930 IF K$=CHR$(28) OR K$="A" THEN NewPick=(NewPick-2+GlyphCount) MOD GlyphCount+1
935 IF K$=CHR$(29) OR K$="D" THEN NewPick=NewPick MOD GlyphCount+1
940 IF K$=CHR$(30) THEN NewPick=MAX(1,NewPick-Cols)
945 IF K$=CHR$(31) THEN NewPick=MIN(GlyphCount,NewPick+Cols)
950 IF K$=CHR$(11) THEN NewPick=MAX(1,NewPick-ChartRows*Cols)
952 IF K$=CHR$(12) THEN NewPick=MIN(GlyphCount,NewPick+ChartRows*Cols)
954 IF K$=CHR$(1) THEN NewPick=1
956 IF K$=CHR$(4) THEN NewPick=GlyphCount
960 PickRow=INT((NewPick-1)/Cols) : NewTop=MAX(PickRow-ChartRows+1,MIN(NewTop,PickRow))
965 IF NewPick<>Pick OR NewTop<>FirstChartRow THEN Pick=NewPick : FirstChartRow=NewTop : Redraw=1
970 RETURN

980 GridLayout:
982 IF SmallGrid THEN Cols=20 : CellW=25 : GlyphInset=8 : GridName$="SMALLFONT" ELSE Cols=16 : CellW=31 : GlyphInset=6 : GridName$="BIGFONT"
984 LastTop=MAX(0,INT((GlyphCount-1)/Cols)+1-ChartRows) : PickRow=INT((Pick-1)/Cols)
986 FirstChartRow=MAX(0,MIN(LastTop,MAX(PickRow-ChartRows+1,MIN(BigTop,PickRow))))
988 ChartWheel=0 : RETURN

1000 ExampleKeys:
1010 WholeRows=FIX(WheelSteps) : WheelSteps=WheelSteps-WholeRows : NewFirst=FirstRow-WholeRows
1020 IF K$=CHR$(30) THEN NewFirst=NewFirst-1
1030 IF K$=CHR$(31) THEN NewFirst=NewFirst+1
1040 IF K$=CHR$(11) THEN NewFirst=NewFirst-RowsShown
1050 IF K$=CHR$(12) THEN NewFirst=NewFirst+RowsShown
1060 IF K$=CHR$(1) THEN NewFirst=0
1070 IF K$=CHR$(4) THEN NewFirst=SampleCount-RowsShown
1130 NewFirst=MAX(0,MIN(SampleCount-RowsShown,NewFirst))
1140 IF FirstRow<>NewFirst THEN FirstRow=NewFirst : Redraw=1
1150 RETURN

1200 Clicked:
1210 ClickX=MOUSEX : ClickY=MOUSEY : ClickPending=1 : RETURN
1220 Scrolled:
1230 IF PageNo=1 THEN ChartWheel=ChartWheel+3*MOUSEWHEEL ELSE WheelSteps=WheelSteps+3*MOUSEWHEEL
1235 RETURN
1240 HandleClick:
1250 IF ClickY>=10 AND ClickY<=40 AND ClickX>=16 AND ClickX<152 THEN PageNo=1 : Redraw=1 : RETURN
1260 IF ClickY>=10 AND ClickY<=40 AND ClickX>=180 AND ClickX<308 THEN PageNo=2 : Redraw=1 : RETURN
1270 IF PageNo<>1 THEN RETURN
1280 C=INT((ClickX-ChartX)/CellW) : R=INT((HEIGHT-1-ClickY-ChartTop+3)/CellH)
1290 IF C>=0 AND C<Cols AND R>=0 AND R<ChartRows THEN
1292   ClickGlyph=(FirstChartRow+R)*Cols+C+1
1294   IF ClickGlyph<=GlyphCount THEN Pick=ClickGlyph : Redraw=1
1296 ENDIF
1300 RETURN

2000 REM Short writing samples for comparing the two fonts side by side.
2030 DATA 14
2040 DATA "Polish","Zażółć gęślą jaźń."
2050 DATA "Czech","Příliš žluťoučký kůň."
2060 DATA "Slovak","Ľúbim ťa. Kŕdeľ, vŕba."
2070 DATA "Romanian","Știință în Iași și Brașov."
2080 DATA "Hungarian","Árvíztűrő tükörfúrógép."
2090 DATA "Croatian","Đakovo, Čakovec, Međimurje."
2100 DATA "Spanish","¡Qué pingüino más pequeño!"
2110 DATA "Danish","Æbletræet står på øen."
2120 DATA "Norwegian Bokmål","Øyvind og Åse plukker blåbær."
2130 DATA "Swedish","Är Öland en ö nära Småland?"
2140 DATA "Icelandic","Þór þvær fötin með mér í dag."
2150 DATA "Faroese","Í Suðuroy sá eg fýra hús."
2160 DATA "Finnish","Yöllä näkyy tähtiä."
2170 DATA "Diacritics","Ă Ě Ů   Ą Ę   Ş Ș   Ţ Ț   Đ Ð"
