# From a paddle to Arkanoid

[Leer en español](README.es.md)

In the first program, you move a paddle with the mouse. By the fourth, you can aim the ball and clear a row of bricks. Each program draws its own sprites and runs on its own.

Start AVL BASIC in the directory that contains `samples` and run the first program.

```basic
RUN "samples/tutorials/arkanoid/01-paddle.bas"
```

Move the mouse inside the graphics window. Press **Esc** to quit. To try the next step, change the filename in the `RUN` command. Lines carried over from the previous step keep their numbers. To try an edit at the `Ready` prompt, use `LOAD` to open the file, type the whole replacement line with its number, then type `RUN`.

## 1. Move the paddle

[01-paddle.bas](01-paddle.bas) · Move the mouse from one edge of the window to the other. Watch what happens when the paddle reaches the edge first.

```basic
20 INK 14 : FRECTANGLE 0,0,63,7
30 Paddle$=SPRITE$(0,0,63,7)
```

Line 10 opens a 640 × 480 screen with a black background. Line 20 draws a solid rectangle 64 pixels wide and 8 pixels high. Line 30 captures it in the string `Paddle$`. That is our first sprite. We can now draw it anywhere. Line 40 gives it a starting position halfway across the screen.

On each pass through the loop, lines 210–220 read `MOUSEX` and subtract 32 to put the centre of the paddle under the pointer. `MAX` and `MIN` keep its left edge between 0 and `WIDTH-64`, so the whole paddle stays on screen. If the pointer leaves the window, the paddle stays where it was.

Lines 230–290 clear the drawing buffer with `CLG OFFSCREEN`, draw the paddle with `SPRITE Paddle$,X,16`, then show the finished frame with `FRAME 60`, up to 60 times a second. **Try it.** Change 16 to 60 on line 240. Which way does the paddle move when you increase Y?

## 2. Set the ball in motion

[02-ball.bas](02-ball.bas) · Leave the mouse still and follow the ball. It bounces off the side walls and the ceiling. For now it passes through the paddle. When it drops out of sight, the next serve starts at the paddle.

Lines 50–60 draw a circle with a radius of 4 pixels and capture the 9 × 9 square around it. Line 280 draws it with `SPRITE Ball$,BX,BY,0`. The final zero makes the black corners of the square transparent.

```basic
70 BX=X+28 : BY=24 : DX=3 : DY=3
```

`BX,BY` are the coordinates of the sprite’s bottom-left corner. `DX,DY` say how far the ball travels each frame. It starts just above the paddle, moving three pixels to the right and three up.

After the frame appears, lines 300–310 look one move ahead. If the ball would cross a side wall, they reverse `DX`. If it would cross the ceiling, they reverse `DY`. Line 400 moves the ball. Line 410 serves it again from the paddle after it falls below the screen. **Try it.** Change `DX=3` to `DX=1` on line 70 and watch the angle of the first serve.

## 3. Return the ball

[03-bounces.bas](03-bounces.bas) · Try sending one return to the left and the next one to the right.

Line 130 enables collisions between sprites with `COLMODE 2`. On lines 240 and 280 we give each sprite an ID as we draw it. The paddle is 100 and the ball is 101. Now we can ask what the ball would hit on its next move.

```basic
320   SPRITE HITTEST Ball$,BX+DX,BY+DY,0,101
330   IF HITID=100 AND DY<0 AND BY>=24 THEN
340     IF BX+4<X+32 THEN DX=-3 ELSE DX=3
350     DY=-DY : BY=24
360   ENDIF
```

`SPRITE HITTEST` tries the new position without drawing the ball. Using the ball’s own ID lets it ignore its current pixels. `HITID` is the ID of the sprite it would hit, or zero if the way is clear.

Line 330 accepts a hit on the paddle when the ball is coming down from above. Line 340 compares the centre of the ball (`BX+4`) with the centre of the paddle (`X+32`). Catch the ball on the left half to send it left. Line 350 puts the ball just above the paddle and reverses its vertical speed so it travels up again.

## 4. Break the bricks

[04-bricks.bas](04-bricks.bas) · Try `N=1` on line 100 to play with a single brick. Once you break it, set `N=8` to play the full row.

Lines 80–90 draw and capture **one** brick, 56 pixels wide and 16 high. Lines 100–120 lay out the row. `RowX` is the starting position that centres it with 16 pixels between bricks. `RowY` sets its height. `BricksLeft` counts the bricks still in play. Each entry in `Broken` starts at zero.

```basic
250   FOR I=1 TO N
260     IF Broken(I)=0 THEN SPRITE Brick$,RowX+(I-1)*72,RowY,0,I
270   NEXT I
```

Every brick uses the same picture, but each copy gets its own ID, `I`. If the first `HITTEST` returns 3, the ball has hit the third brick. Lines 365–370 save that ID in `I`, set `Broken(I)=1` and subtract one from `BricksLeft`. On the next frame, line 260 skips that brick. The ball can now pass through the gap.

```basic
375     SPRITE HITTEST Ball$,BX+DX,BY,0,101
380     IF HITID=I THEN DX=-DX ELSE DY=-DY
```

Notice the second `HITTEST`. This time we try moving only along X, keeping the ball at its current height. If that alone would hit the same brick, the ball has reached its side and we reverse `DX`. Otherwise we reverse `DY`. When the last brick breaks, `BricksLeft` reaches zero. The loop on line 200 ends and lines 500–515 show the victory message until you press a key.

## Find the same ideas in the full game

Open [g-arkanoid.bas](../../g-arkanoid.bas) at line **1470**. You already know what that `SPRITE HITTEST` is asking and how to use the ID it returns. Then look at lines **1250–1280**. The full game saves and restores the small patches of background under the paddle and ball. That lets them move over a detailed scene without clearing the whole screen each frame.
