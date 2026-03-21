100 REM Mandelbrot set
110 XMI=-2.5 : XMA=1 : YMI=-1.3125 : YMA=1.3125
115 W=WIDTH : H=HEIGHT
120 ZoomFactor = 8 : MaxIter=50
150 DEG
160 CLG : SCALE XMI,XMA,YMI,YMA : t1=TIME
165 RX=(XMA-XMI)/W : RY=(YMA-YMI)/H
170 FOR x=0 TO W-1
180 r=XMI+x*RX
190 FOR y=0 TO H-1
200 i=YMA-y*RY
210 zr=zi=0
220 FOR n=0 TO MaxIter
230 tmp=zr*zr-zi*zi+r : zi=(zr+zr)*zi+i : zr=tmp
240 IF zr*zr+zi*zi>4 THEN 260
250 NEXT n
260 IF n>MaxIter THEN INK 0 ELSE INK (n MOD 29)+2
270 PLOT r,i
280 NEXT y
290 FRAME
300 NEXT x
310 PRINT "Elapsed time:";TIME-t1
320 PAUSE
330 REM Zoomed view centered on the pointer coordinates
340 cx=XPOS : cy=YPOS
350 MaxIter=MaxIter+20*LOG10(ZoomFactor)
360 an=(XMA-XMI)/ZoomFactor : al=an*1.333 'Force a 4:3 aspect ratio
370 XMI=cx-an/2 : XMA=cx+an/2 : YMI=cy-al/2 : YMA=cy+al/2
380 GOTO 160

