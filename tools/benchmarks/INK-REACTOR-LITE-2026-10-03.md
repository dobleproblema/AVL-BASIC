# Ink Reactor Lite: tres variantes — 3 de octubre de 2026

Informe histórico de la variante Lite, descartada. La demo elegida se llama
ahora [`g-smoke.bas`](../../samples/g-smoke.bas), **Smoke**. Los nombres y comandos
siguientes corresponden a las muestras utilizadas en aquella comparación.

En aquella comparación, `samples/g-ink-reactor-lite.bas` reducía aproximadamente
dos tercios del tiempo por fotograma frente a la optimizada anterior, con sus
ajustes rápidos iniciales. Las fuentes `g-ink-reactor-original.bas`,
`g-ink-reactor.bas` y `g-ink-reactor-lite.bas` eran archivos independientes.
Esas variantes y la antigua guía `samples/INK-REACTOR-COMPARACION.md` ya no
forman parte del checkout actual. Las cifras no describen el rendimiento de
la muestra actual Smoke, que contiene cambios posteriores.

Para repetir la tanda hay que restaurar las fuentes históricas y cotejar los
hashes al final del informe. `tools/compare_ink_reactor.ps1` requiere las fuentes
original y Lite en `samples`, además de la Smoke actual; el lanzador no restaura
ninguna fuente y se detiene si faltan. Esa vista simultánea tampoco reproduce
una medición aislada histórica.

## Qué se aproxima

La tinta RGB sigue en la cuadrícula seleccionada. El campo de velocidad,
su advección, la presión y la vorticidad se calculan en una malla menor:
24 × 15 para tinta de 96 × 60; 32 × 20 para tinta de 128 × 80. Son dieciséis
veces menos celdas para esas operaciones, aunque el trabajo total no se reduce
en esa proporción: transportar y dibujar la tinta sigue teniendo coste.
La velocidad se interpola bilinealmente al transportar cada celda de tinta.

La conversión de coordenadas y unidades mantiene la geometría de la pantalla.
El impulso del pincel se deposita en la malla gruesa, con un radio mínimo de
una celda para que un pincel pequeño no quede entre centros sin agitar nada.
La huella y la cantidad de tinta inyectada conservan las fórmulas anteriores.
Las paredes reflejan la velocidad normal en las dos mallas.

Se mantiene una proyección SOR de cuatro barridos, con omega 1,5. El
confinamiento usa deliberadamente `EPS*DT`, en lugar de `EPS*CF*DT`: refuerza
la fuerza en unidades de pantalla por `NX/CX`, cuatro veces a 96 y 128 con
el ajuste inicial. Compensa visualmente parte de la difusión numérica; no
recupera los remolinos demasiado pequeños para la nueva malla ni conserva
la misma evolución física. La tecla J aumenta el detalle del movimiento y
reinicia; el indicador FLOW muestra la resolución efectiva tras el redondeo.

El modo rápido dibuja una celda por muestra de tinta, sin suavizado. B activa
un remuestreo bilineal de aproximadamente 1,5 veces por eje, frente a las dos
veces de la versión anterior. A 128 × 80 dibuja 192 × 120 muestras, un 43,75 %
menos que 256 × 160. El color se transforma después de interpolar densidades.
No se omiten fotogramas de simulación o presentación. La pérdida visible es
menor detalle de remolinos y mayor pixelación en el modo rápido; no se afirma
que la imagen sea indistinguible.

## Medición en Windows con ventana

Intel Core i7-13700K, mismo ejecutable existente para ambas versiones. Un par
externo de calentamiento y cuatro pares medidos AB/BA por tamaño y modo;
cada proceso tiene cinco pasos internos de calentamiento y 120 medidos.
Escenario automático, `DT=1/30`, vorticidad activada, HUD desactivado y FPS
sin límite. Exportaciones fuera del intervalo medido. Mediana ± MAD de FPS:

| Cuadrícula | Anterior suavizada | Lite rápida | Aceleración |
| --- | ---: | ---: | ---: |
| 96 × 60 | 56,259 ± 0,107 | 165,306 ± 0,902 | 2,938× |
| 128 × 80 | 31,922 ± 0,067 | 94,941 ± 0,452 | 2,974× |

La prueba separada con suavizado activo en ambas versiones da:

| Cuadrícula | Anterior, suavizado 2× | Lite, suavizado 1,5× | Aceleración |
| --- | ---: | ---: | ---: |
| 96 × 60 | 55,532 ± 0,038 | 109,563 ± 0,454 | 1,973× |
| 128 × 80 | 31,843 ± 0,044 | 63,167 ± 0,128 | 1,984× |

Son mediciones locales de capacidad sin límite; la muestra interactiva arranca
limitada a 60 FPS y F quita ese límite. No se ha medido en el Surface con batería.
Las mediciones con tres ventanas simultáneas no son comparables a estas tandas.

## Medición sin ventana

Tanda separada, rasterización activa, mismos ajustes rápidos de cada fuente,
un par externo de calentamiento y dos pares medidos AB/BA por tamaño:

| Cuadrícula | Anterior, FPS | Lite, FPS | Aceleración |
| --- | ---: | ---: | ---: |
| 40 × 25 | 310,767 ± 1,934 | 760,478 ± 0,964 | 2,447× |
| 64 × 40 | 125,702 ± 0,101 | 371,018 ± 1,266 | 2,952× |
| 80 × 50 | 82,037 ± 0,339 | 231,093 ± 0,465 | 2,817× |
| 96 × 60 | 56,539 ± 0,025 | 167,998 ± 0,646 | 2,971× |
| 128 × 80 | 32,260 ± 0,023 | 96,983 ± 0,423 | 3,006× |

## Verificación y evidencia

Las tres tandas suman 70 procesos. Cada variante reproduce exactamente sus
PNG, campos CSV y salida sin temporizador entre repeticiones. Se comprobaron
484.156 registros de celdas, todos finitos, con tinta en [0,16]; el máximo fue
12,3156. El CSV usa la serialización de BASIC WRITE, no compara los bits internos
de cada número. Los campos finales coinciden entre los modos de dibujo de
cada variante: el suavizado no cambia la simulación. Los campos entre versiones
son distintos, como corresponde a la aproximación introducida.

Otras 16 ejecuciones funcionales verificaron los controles B/J/P/V/C/R,
pausa y ciclo Q en los cinco tamaños, además del pincel automático de prueba.
La pausa conservó exactamente los campos; C eliminó la tinta sin cambiar
la velocidad. Se revisaron finitud, rango RGB y fronteras en ambas mallas.
Los escenarios automático y de pincel intenso completaron 30,25 segundos
simulados a `DT=0.05`, dos veces cada uno. Sus estados finales y las capturas
de controles fueron correctos y deterministas; no se inspecciona cada celda
en cada paso ni se afirma estabilidad para cualquier ejecución posible.

WSL/Linux completó el caso de pincel a 128 × 80, 185 pasos y sin ventana,
con campos finitos, densidades válidas y fronteras correctas. Sus campos y
PNG no son idénticos a Windows: en este caso la máxima diferencia absoluta
de densidad fue 0,00175 y de una componente de velocidad 0,908 celdas/s.
Es una comprobación funcional, no una comparación de rendimiento Linux.

El lanzador se probó a 128 × 80 incluso partiendo de `AVL_BASIC_WINDOW=0`:
abrió tres ventanas gráficas identificadas, con procesos que respondían,
y restauró el valor anterior del entorno. Sus copias temporales y PIDs
quedan documentados en el `manifest.json` de cada lanzamiento.

Los informes y fuentes congeladas están en `target/ink-lite-20261003/`:
`final-window/report.json`, `final-smooth-window/report.json`,
`final-headless/report.json`, `final-evidence.json`,
`functional-validation/validation.json` y `wsl-final/validation.json`.

## Identidad y reproducción

| Archivo | SHA-256 |
| --- | --- |
| Original conservada | `f39b830fb6bf5749d575afc597b02267cb8a3bcc65ed7d2f684d05fdd434eaaf` |
| Optimizada anterior, intacta | `562028938473ffd8691692db16dabd4934495a0bdddd77af31eaa8f8c38612b2` |
| Nueva Lite | `463f41f9a63436048c4f7931c3d9d2bb91d666134843c2c01b562689b556ec48` |
| Ejecutable Windows | `c6ff712fad985071490ece9a1d4d151b857096475ae239c0aad7a4c566192da5` |

Desde la raíz del repositorio, con una carpeta de salida nueva:

```powershell
python tools/benchmarks/ink_reactor.py --baseline samples/g-ink-reactor.bas --candidate samples/g-ink-reactor-lite.bas --output target/ink-lite-repeat --grids 96,128 --frames 120 --runs 4 --warmups 1 --window 1 --source-smoothing --allow-differences
```

`--source-smoothing` respeta el modo inicial de cada muestra. Para la tanda
suavizada, sustituirlo por `--smooth 1`; cambiar también la carpeta de salida.
Para la tanda sin ventana, usar `--window 0 --grids 40,64,80,96,128 --runs 2`.
`--allow-differences` permite que las variantes difieran, pero sigue exigiendo
determinismo en las repeticiones de cada una.
