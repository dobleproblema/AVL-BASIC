100 REM Iterative Levy C curve (NO recursion)
110 DEG
120 SCREEN : CLG : MODE 640
130 P=12          'Depth
140 L=4           'Step length (pixels)
150 S$="F"        'Sequence
160 FOR N=1 TO P  'Iterative L-system expansion
170   N$ = ""     'New sequence
180   FOR K=1 TO LEN(S$)
190     C$=MID$(S$,K,1)
200     IF C$="F" THEN N$=N$+"+F--F+" ELSE N$=N$+C$
210   NEXT K
220   S$=N$
230 NEXT N
240 REM Draw
250 A=0           'Initial direction (0° -> right)
260 MOVE 190,140  'Starting point
270 FOR K=1 TO LEN(S$)
280   C$=MID$(S$, K, 1)
290   IF C$="F" THEN DRAWR L*COS(A),L*SIN(A):GOTO 320
300   IF C$="+" THEN A=A+45:GOTO 320
310   IF C$="-" THEN A=A-45
320 NEXT K
330 END

