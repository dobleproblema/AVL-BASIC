100 PTS=10000 ' PTS=1000 for Python version
110 PI2=PI/2
120 ORIGIN WIDTH\2,HEIGHT\2
130 INK &H44ccff
140 DIM x(PTS),y(PTS),k(PTS),e(PTS)
150 FOR i=1 TO PTS
160 x(i)=i MOD 200
170 y(i)=i/55
180 k(i)=9*COS(x(i)/8)
190 e(i)=y(i)/8-12.5
200 NEXT i
210 t=0
220 CLG
230 FOR i=1 TO PTS
240 k=k(i) : e=e(i)
250 d=(k*k+e*e)/99+SIN(t)/6+0.5
260 IF e THEN a=ATN(k/e) ELSE a=PI2
270 q=99-e*SIN(a*7)/d+k*(3+COS(d*d-t)*2)
280 c=d/2+e/69-t/16
290 PLOT q*SIN(c),(q+19*d)*COS(c)
300 NEXT i
310 t=t+0.1
320 FRAME
330 GOTO 220
