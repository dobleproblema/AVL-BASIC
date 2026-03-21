100 SCREEN : MODE 640 : BIGFONT
110 PAPER 2 : CLG
115 COLS=WIDTH\16
116 MSG1$="Drag the mouse with the left button."
117 MSG2$="Right-click to exit."
120 REM Draw a sprite and store it in cara$
130 INK 3 : FRECTANGLE 160,80,240,160
140 INK 4 : FCIRCLE 200,120,30
150 cara$ = SPRITE$(150,70,250,170)
160 REM Draw the background and display it
170 CLG
180 DRAW 0,0,640,480,15
190 MOVE 1,20 : FILL 18 : MOVE 20,1 : FILL 12
200 LOCATE (COLS-LEN(MSG1$))\2,0 : DISP MSG1$,4,-1
210 LOCATE (COLS-LEN(MSG2$))\2,29 : DISP MSG2$,4,-1
220 FRAME
230 REM Mouse events and capture loop
240 ON MOUSE LEFTDRAG GOSUB 280 'Drag with the left mouse button held down
250 ON MOUSE RIGHTUP GOSUB 350 'Right button exits the program
260 GOTO 260
270 REM Capture the background before drawing the sprite
280 x=MOUSEX : y=MOUSEY
290 fondo$ = SPRITE$(x,y,x+100,y+100)
300 SPRITE cara$,x,y,4 'Color 4 in the sprite is transparent
310 FRAME
320 REM Restore the background, but do not show it until the next FRAME
330 SPRITE fondo$,x,y
340 RETURN
350 SCREEN CLOSE
360 END

