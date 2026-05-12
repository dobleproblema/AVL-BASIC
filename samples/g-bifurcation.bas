100 REM =========================================================
105 REM  LOGISTIC MAP  —  λ -> RAINBOW • AXES • CLICK TO ZOOM
110 REM  © 2025  José Antonio Ávila
115 REM =========================================================
120 DEG : SCREEN : PENWIDTH 1
125 B=40                        'Margin in pixels (BORDER from SCALE)
130 AN=WIDTH-2*B                'Usable width
140 R0=2.5 : R1=4               'Initial window in r
145 IT=1000 : SK=100 : MS=120
150 GOTO 160                    'Main loop
155 REM Main loop
160 CLG
165 GOSUB 310                   'Draw axes
170 GOSUB 210                   'Draw diagram
175 PAUSE                       'Click inside graph = zoom, click outside = end
180 CX=XPOS : IF CX<R0 OR CX>R1 THEN END
190 RV=CX                       'r clicked
195 W=(R1-R0)/4
200 R0=RV-W/2 : R1=RV+W/2
205 GOTO 160                    'Redraw with the new window
210 REM Bifurcation diagram (column by column)
215 FOR PX=0 TO AN-1
220   RR=R0+(R1-R0)*PX/AN
225   X=0.5 : LSUM=0
230   FOR I=1 TO IT
235     X=RR*X*(1-X)
240     IF I<=SK THEN 255
245       D=RR*(1-2*X) : IF D=0 THEN D=0.00001
250       LSUM=LSUM+LOG(ABS(D))
255   NEXT I
260   L=LSUM/(IT-SK)
265   GOSUB 350                 'λ -> INK (rainbow)
270   X=0.5
275   FOR I=1 TO IT
280     X=RR*X*(1-X)
285     IF I>IT-MS THEN PLOT RR,X : FRAME

295   NEXT I
300 NEXT PX
305 RETURN
310 REM X (r) and Y (x) axes
312 SCALE R0,R1,0,1,B
315 CROSSAT R0,0
320 XTIC=(R1-R0)/8
325 INK 1
330 XAXIS XTIC,R0,R1,0,0
335 YAXIS 0.25,0,1,0
345 RETURN
350 REM λ (-2...0.5)  ->  INK RGB (HSV)
355 T=(L+2)/2.5 : IF T<0 THEN T=0 : IF T>1 THEN T=1
360 H=270*(1-T)
365 H6=H/60 : HI=INT(H6)
370 F=H6-HI
375 Q=1-F : T2=F
380 ON HI+1 GOTO 385, 390, 395, 400, 405, 410
385 CR=1 : CG=T2 : CB=0 : GOTO 415
390 CR=Q : CG=1 : CB=0 : GOTO 415
395 CR=0 : CG=1 : CB=T2 : GOTO 415
400 CR=0 : CG=Q : CB=1 : GOTO 415
405 CR=T2 : CG=0 : CB=1 : GOTO 415
410 CR=1 : CG=0 : CB=Q
415 INK INT(CR*255),INT(CG*255),INT(CB*255)
420 RETURN

