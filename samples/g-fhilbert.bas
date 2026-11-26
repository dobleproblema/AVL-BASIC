100 REM Hilbert curve
105 MODE 1024 : DEG : SMALLFONT
110 P=0 : WheelP=P : B=50 : H=HEIGHT-2*B
125 Redraw:
130 S=2^(P+1)-1 : XW=S*(WIDTH-2*B)/H : X0=(XW-S)/2
135 SCALE 0,XW,0,S,B : CLG OFFSCREEN : T1=TIME
140 S$="+LF-XFX-FL+" 'Starting path
150 REM Join four copies of the path, reversing the turns in the outer two.
160 FOR I=1 TO P
170   GOSUB MirrorTurns
180   S$="+"+R$+"F-"+S$+"F"+S$+"-F"+R$+"+"
190 NEXT I
200 REM Follow the path: F draws one step, + and - turn left and right.
210 A=0 : MOVE X0,0
220 FOR K=1 TO LEN(S$)
230   C$=MID$(S$,K,1)
240   IF C$="F" THEN DRAWR COS(A),SIN(A)
250   IF C$="+" THEN A=A+90 ELSE IF C$="-" THEN A=A-90
260 NEXT K
270 Elapsed=TIME-T1
280 GOSUB ShowStatus : GOSUB WaitForWheel
290 GOTO Redraw
300 MirrorTurns:
305 R$=S$
310 FOR C=1 TO LEN(R$)
320   C$=MID$(S$,C,1)
330   IF C$="+" THEN MID$(R$,C,1)="-" ELSE IF C$="-" THEN MID$(R$,C,1)="+"
340 NEXT C
350 RETURN
400 ShowStatus:
410 SCALE : LDIR 90
420 MOVE 8,8 : LABEL "Mouse wheel: up P+1 / down P-1"
430 MOVE WIDTH-24,8 : LABEL "ESC to exit"
440 IF Elapsed<0.1 THEN Duration$=DEC$(Elapsed*1000,"0.00")+" ms" ELSE Duration$=DEC$(Elapsed,"0.000")+" s"
450 Status$="P="+STR$(P)+"    Time: "+Duration$
460 LDIR 0 : MOVE (WIDTH-8*LEN(Status$))/2,23 : LABEL Status$ : FRAME
470 RETURN
500 WaitForWheel:
510 ON MOUSE WHEEL GOSUB ChangeOrder
520 WHILE ROUND(WheelP)=P
530   IF INKEY$=CHR$(27) THEN SCREEN CLOSE : END
540   FRAME 30
550 WEND
560 ON MOUSE WHEEL GOSUB 0
570 P=ROUND(WheelP) : RETURN
600 ChangeOrder:
610 WheelP=MAX(0,MIN(7,WheelP+MOUSEWHEEL))
620 RETURN
