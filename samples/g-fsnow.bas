100 REM Koch snowflake. L-system technique
110 DEG : SCREEN : MODE 640
120 P=4 : L=5   '(1, 117), (2, 41), (3, 13), (4, 5)
130 S$="F++F++F"  'Initial sequence
140 FOR N=1 TO P  'Iterative L-system expansion
150   N$ = ""     'New sequence
160   FOR K=1 TO LEN(S$)
170     C$=MID$(S$,K,1)
180     IF C$="F" THEN N$=N$+"F-F++F-F" ELSE N$=N$+C$
190   NEXT K
200   S$=N$
210 NEXT N
220 REM Draw
230 A=0           'Initial direction (0° -> right)
240 MOVE 135,135  'Starting point
250 FOR K=1 TO LEN(S$)
260   C$=MID$(S$, K, 1)
270   IF C$="F" THEN DRAWR L*COS(A),L*SIN(A) ELSE IF C$="+" THEN A=A+60 ELSE A=A-60
280 NEXT K
290 END

