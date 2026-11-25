100 REM Mandelbrot set
105 PRINT "Wheel: zoom at the pointer. Left click: zoom in and recenter. Ctrl+C: stop."
110 XMI=-2.5 : XMA=1 : YMI=-1.3125 : YMA=1.3125
115 W=WIDTH : H=HEIGHT
120 MaxIter=50
150 DEG : DI
155 ON MOUSE LEFTDOWN GOSUB ClickZoom : ON MOUSE WHEEL GOSUB WheelZoom
160 Redraw:
161 NextXMI=XMI : NextXMA=XMA : NextYMI=YMI : NextYMA=YMA : ZoomPending=0
162 CLG : SCALE XMI,XMA,YMI,YMA : t1=TIME
163 EI
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
320 IF ZoomPending=0 THEN PAUSE 10
325 IF ZoomPending=0 THEN 320
330 DI
340 XMI=NextXMI : XMA=NextXMA : YMI=NextYMI : YMA=NextYMA
350 MaxIter=MAX(50,50+50*LOG10(3.5/(XMA-XMI)))
380 GOTO Redraw
400 ClickZoom:
410 GOSUB PointerFraction : Factor=0.125 : AnchorX=0.5 : AnchorY=0.5
420 GOSUB QueueZoom : RETURN
500 WheelZoom:
510 GOSUB PointerFraction : Factor=1.5^(-MAX(-64,MIN(64,MOUSEWHEEL)))
520 AnchorX=PX : AnchorY=PY
530 GOSUB QueueZoom : RETURN
600 PointerFraction:
610 PX=(MOUSEX-XMI)/(XMA-XMI) : PY=(MOUSEY-YMI)/(YMA-YMI)
620 RETURN
700 QueueZoom:
710 ' Compose input received during the current redraw in the pending view.
720 OldW=NextXMA-NextXMI : OldH=NextYMA-NextYMI
730 cx=NextXMI+PX*OldW : cy=NextYMI+PY*OldH
740 NewW=MAX(1E-12,MIN(350,OldW*Factor)) : NewH=OldH*(NewW/OldW)
750 NextXMI=cx-AnchorX*NewW : NextXMA=NextXMI+NewW
760 NextYMI=cy-AnchorY*NewH : NextYMA=NextYMI+NewH : ZoomPending=1
770 RETURN
