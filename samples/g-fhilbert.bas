100 REM Hilbert curve
110 CLG : DEG : T1=TIME
120 P=5 : B=50 : H=HEIGHT-2*B : S=2^(P+1)-1
125 XW=S*(WIDTH-2*B)/H : X0=(XW-S)/2
130 SCALE 0,XW,0,S,B
140 S$="+LF-XFX-FL+" 'Basic motif
150 REM Build the string
160 FOR I=1 TO P
170   GOSUB 300 'Generate R$, a copy of S$ with swapped signs
180   S$="+"+R$+"F-"+S$+"F"+S$+"-F"+R$+"+"
190 NEXT I
200 REM Draw
210 A=0 'Initial angle
220 MOVE X0,0 'Initial position
230 FOR K=1 TO LEN(S$)
240   C$=MID$(S$, K, 1)
250   IF C$="F" THEN DRAWR COS(A),SIN(A) : GOTO 270
260   IF C$="+" THEN A=A+90 ELSE IF C$="-" THEN A=A-90
270 NEXT K
280 PRINT "Elapsed time:";TIME-T1
290 END
300 R$=S$
310 FOR C=1 TO LEN(R$)
320 C$=MID$(S$,C,1)
330 IF C$="+" THEN MID$(R$,C,1)="-" ELSE IF C$="-" THEN MID$(R$,C,1)="+"
340 NEXT
350 RETURN

