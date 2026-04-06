100 REM Color input formats demo
110 SCREEN : MODE 640 : PAPER RGB$(18,22,28) : CLG : SMALLFONT
120 Q$=CHR$(34)
130 MOVE 20,460 : GDISP "Supported color formats for INK, PAPER, and drawing commands.",1,-1
140 MOVE 40,430 : GDISP "PAPER "+Q$+"0,64,96"+Q$+"      [RGB string]",1,"0,64,96"
150 MOVE 340,430 : GDISP "PAPER 6291552   [Packed RGB, dec]",1,6291552
160 MOVE 20,398 : GDISP "Swatches below use different valid forms with INK (Also valid for PAPER).",1,-1
170 MOVE 40,368 : GDISP "INK 2          [32 color palette]",1,-1
180 INK 2 : FRECTANGLE 40,300,300,350
190 MOVE 340,368 : GDISP "INK &H3366CC    [Packed RGB, hex]",1,-1
200 INK &H3366CC : FRECTANGLE 340,300,600,350
210 MOVE 40,278 : GDISP "INK "+Q$+"red"+Q$+"           [Named color]",1,-1
220 INK "red" : FRECTANGLE 40,210,300,260
230 MOVE 340,278 : GDISP "INK 96,96,96       [Unpacked RGB]",1,-1
240 INK 96,96,96 : FRECTANGLE 340,210,600,260
250 MOVE 40,188 : GDISP "INK RGB(96,0,96)   [RGB() Output]",1,-1
260 INK RGB(96,0,96) : FRECTANGLE 40,120,300,170
270 MOVE 340,188 : GDISP "INK RGB$(24576)   [RGB$() Output]",1,-1
280 INK RGB$(0,96,0) : FRECTANGLE 340,120,600,170
290 MOVE 20,110 : GDISP "Drawing commands expect color as a single argument. For example:",1,-1
300 MOVE 300,YPOS-2 : GDISP "______","red",-1
310 INK 1 : RECTANGLE 55,30,160,70
320 FILL 110,60,"gold"
330 MOVE 40,89 : GDISP "FILL 110,60,"+Q$+"gold"+Q$,1,-1 : MOVE 136,YPOS-2 : GDISP "______","red",-1
340 INK 1 : RECTANGLE 260,30,360,70
350 FILL 280,60,RGB("70,160,255")
360 MOVE 205,89 : GDISP "FILL 280,60,RGB(70,160,255)",1,-1 : MOVE 300,YPOS-2 : GDISP "_______________","red",-1
370 FCIRCLE 520,50,21,8
380 MOVE 445,89 : GDISP "FCIRCLE 500,58,30,8",1,-1 : MOVE 589,YPOS-2 : GDISP "_","red",-1
390 MOVE 20,18 : GDISP "But not FILL 280,60,70,160,255: FILL only has one color parameter.",1,-1
400 MOVE 181,YPOS-2 : GDISP "__ ___ ___","red",-1
