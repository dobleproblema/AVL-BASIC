# De la pala a Arkanoid

[Read in English](README.md)

En el primer programa moverás una pala con el ratón. En el cuarto podrás apuntar con ella y romper una fila de ladrillos. Los cuatro dibujan sus propios sprites y se pueden ejecutar por separado.

Para empezar, abre AVL BASIC en la carpeta que contiene `samples` y ejecuta el primer programa.

```basic
RUN "samples/tutorials/arkanoid/01-paddle.bas"
```

Para probar otro paso, cambia el nombre del fichero en el mismo `RUN`. Mueve el ratón dentro de la ventana y pulsa **Esc** para salir. Las líneas que se conservan de un paso al siguiente mantienen su número. Si quieres experimentar desde `Ready`, carga el fichero con `LOAD`, teclea la línea modificada con su número y escribe `RUN`.

## 1. Mueve la pala

[01-paddle.bas](01-paddle.bas) · Mueve el ratón de extremo a extremo. La pala sigue el puntero y se detiene entera en cada borde.

```basic
20 INK 14 : FRECTANGLE 0,0,63,7
30 Paddle$=SPRITE$(0,0,63,7)
```

La línea 10 abre una pantalla de 640 × 480 con fondo negro. La 20 dibuja un rectángulo de 64 × 8 píxeles y la 30 lo captura en la cadena `Paddle$` («pala»). Ya tenemos un sprite y podemos estamparlo tantas veces como queramos. La línea 40 coloca la pala en el centro para empezar.

En cada vuelta del bucle, las líneas 210–220 leen `MOUSEX` y restan 32 para que el puntero quede en el centro de la pala. `MAX` y `MIN` mantienen el extremo izquierdo entre 0 y `WIDTH-64`. Si sacas el puntero de la ventana, la pala conserva su posición.

Las líneas 230–290 preparan la imagen sobre un fondo limpio (`CLG OFFSCREEN`), dibujan la pala (`SPRITE Paddle$,X,16`) y muestran el resultado (`FRAME 60`, hasta 60 veces por segundo). **Prueba** a cambiar el 16 de la línea 240 por 60. ¿En qué dirección se mueve la pala al aumentar su coordenada Y?

## 2. Pon una pelota en movimiento

[02-ball.bas](02-ball.bas) · Deja el ratón quieto y sigue la pelota. Rebota en los laterales y en el techo. Por ahora atraviesa la pala y vuelve a salir cuando se pierde por abajo.

Las líneas 50–60 dibujan un círculo de radio 4 y capturan el cuadrado de 9 × 9 píxeles que lo contiene. La línea 280 lo dibuja con `SPRITE Ball$,BX,BY,0`. El último cero hace transparentes las esquinas negras del cuadrado.

```basic
70 BX=X+28 : BY=24 : DX=3 : DY=3
```

`BX,BY` sitúan la esquina inferior izquierda del sprite. `DX,DY` dicen cuánto se moverá en cada fotograma. Aquí empieza justo encima de la pala y avanza tres píxeles a la derecha y tres hacia arriba.

Tras mostrar la imagen, las líneas 300–310 miran dónde caería la pelota en el siguiente paso. Si se saldría por un lateral, cambian el signo de `DX`. Si se saldría por el techo, cambian el de `DY`. La línea 400 aplica el movimiento y la 410 vuelve a sacar la pelota sobre la pala cuando desaparece por abajo. **Prueba** a cambiar solo el primer `DX=3` por `DX=1` en la línea 70 y observa la trayectoria del primer saque.

## 3. Devuelve la pelota con la pala

[03-bounces.bas](03-bounces.bas) · Intenta devolver una pelota hacia la izquierda y la siguiente hacia la derecha.

En la línea 130, `COLMODE 2` activa los choques entre sprites. En las líneas 240 y 280 añadimos un identificador al dibujar cada objeto. El 100 es la pala y el 101 es la pelota. Así podemos preguntar qué tocaría la pelota si diera su siguiente paso.

```basic
320   SPRITE HITTEST Ball$,BX+DX,BY+DY,0,101
330   IF HITID=100 AND DY<0 AND BY>=24 THEN
340     IF BX+4<X+32 THEN DX=-3 ELSE DX=3
350     DY=-DY : BY=24
360   ENDIF
```

`SPRITE HITTEST` prueba la nueva posición sin dibujarla. Usar el mismo 101 permite que la pelota ignore sus propios píxeles. `HITID` indica qué ha tocado y vale cero si el camino está libre.

La línea 330 acepta el choque con la pala cuando la pelota viene cayendo desde arriba. La 340 compara el centro de la pelota (`BX+4`) con el de la pala (`X+32`). Para lanzar a la izquierda, recibe la pelota con la mitad izquierda. La 350 la sitúa justo sobre la pala e invierte su velocidad vertical para que vuelva a subir.

## 4. Rompe los ladrillos

[04-bricks.bas](04-bricks.bas) · Prueba primero con `N=1` en la línea 100. Cuando rompas ese ladrillo, vuelve a poner `N=8`. La misma pelota puede romper cualquiera de los ocho.

Las líneas 80–90 dibujan y capturan **un** ladrillo de 56 × 16 píxeles. Las líneas 100–120 preparan la fila. `RowX` indica dónde empieza para que quede centrada, con 16 píxeles entre ladrillos. `RowY` fija su altura. `BricksLeft` cuenta los que siguen en juego. Cada elemento de `Broken` empieza en cero.

```basic
250   FOR I=1 TO N
260     IF Broken(I)=0 THEN SPRITE Brick$,RowX+(I-1)*72,RowY,0,I
270   NEXT I
```

Todos usan el mismo dibujo, pero cada copia lleva su propio número `I`. Si el primer `HITTEST` devuelve 3, la pelota ha dado al tercer ladrillo. Las líneas 365–370 guardan ese número en `I`, ponen `Broken(I)=1` y descuentan uno de `BricksLeft`. En el siguiente fotograma la línea 260 se salta ese ladrillo y queda un hueco por el que ya puede pasar la pelota.

```basic
375     SPRITE HITTEST Ball$,BX+DX,BY,0,101
380     IF HITID=I THEN DX=-DX ELSE DY=-DY
```

Fíjate en el segundo `HITTEST`. Esta vez cambiamos solo X y mantenemos la altura actual. Si así ya chocamos con el mismo ladrillo, hemos dado en un lateral e invertimos `DX`. En otro caso invertimos `DY`. Al romper el último, `BricksLeft` llega a cero, termina el bucle de la línea 200 y las líneas 500–515 muestran el mensaje de victoria hasta que pulses una tecla.

## Abre ahora el Arkanoid completo

En [g-arkanoid.bas](../../g-arkanoid.bas), busca primero la línea **1470**. Ya conoces esa pregunta sobre la próxima posición de la pelota y ya sabes qué hacer con el número que devuelve. Luego mira las líneas **1250–1280**. El juego completo guarda y repone los trocitos de fondo que había bajo la pala y la pelota. Así puede moverlas sobre un decorado sin borrar la escena entera en cada fotograma.
