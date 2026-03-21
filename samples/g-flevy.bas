100 REM Levy curve. Recursion simulation
110 DEG : CLG
120 P=10 : L=7
130 DIM C(20), A(20)
140 N=0 : A(0)=0
150 X=(WIDTH-38*L)/2+7*L : Y=(HEIGHT-62*L)/2+15*L
160 MOVE X,Y
170 'Main loop
180 GOSUB 210
190 IF N>=0 THEN 180
200 END
210 REM Levy curve subroutine. One phase per call
220 IF N<>P THEN 280
230   X=X+L*COS(A(N))
240   Y=Y+L*SIN(A(N))
250   DRAW X,Y
260   N=N-1
270   RETURN
280 ON C(N)+1 GOTO 290, 340, 380, 430
290 REM Phase 0: First half
300 C(N)=1
310 A(N+1) = A(N)
320 N=N+1
330 RETURN
340 REM Phase 1: +90 turn
350 C(N)=2
360 A(N)=(A(N)+90) MOD 360
370 RETURN
380 REM Phase 2: Second half
390 C(N)=3
400 A(N+1)=A(N)
410 N=N+1
420 RETURN
430 REM Phase 3: -90 turn and restart
440 A(N)=(A(N)+270) MOD 360
450 C(N)=0
460 N=N-1
470 RETURN

