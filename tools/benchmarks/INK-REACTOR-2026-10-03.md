# Ink Reactor: optimización del ejemplo — 3 de octubre de 2026

Nombre actual de la demo: [`g-smoke.bas`](../../samples/g-smoke.bas), **Smoke**.
Este informe conserva el nombre y las evidencias de las mediciones originales.

**Alcance histórico.** Las cifras y hashes siguientes corresponden a las fuentes
medidas el 3 de octubre, antes de otros cambios posteriores de la muestra actual,
incluida su relación de aspecto. No describen los FPS ni la equivalencia de la
demo actual. Para repetir una tanda histórica hay que restaurar exactamente las
fuentes identificadas por los hashes de esa tanda; no basta con el clon actual.

Actualización posterior: se eligió esta simulación intermedia y se simplificaron
sus controles y HUD. El algoritmo y los parámetros físicos se conservan; los
resultados de rendimiento siguientes corresponden a la interfaz anterior.

Con ventana real en Windows, la cuadrícula de **128 × 80 pasa de 15,02 a
31,90 FPS**: **2,12 veces más rendimiento**, con un tiempo por fotograma de
66,57 → 31,35 ms (−52,91 %). Se conservan la resolución de la simulación, los
tres canales de tinta, la vorticidad y la interpolación bilineal de presentación.
El cambio está en el programa BASIC; ambas versiones usan el mismo ejecutable.

## Qué cambia

### Limpieza posterior de la interfaz

La muestra principal pasó de 214 a 184 líneas numeradas: funcionamiento siempre
automático, sin ratón, selección de color ni controles A/P/V/F/1–5/C. Conserva
Q para resolución, B para suavizado, pausa, reinicio, ocultar HUD y salida.
`ITER=4`, `VORT=1` y `CAP=60` quedan al inicio para editar en el código; CAP
expresa ahora directamente el límite de FPS. El encabezado muestra resolución,
SMOOTH y FPS. Se eliminaron los temporizadores y la segunda línea informativa.

La comparación con paso fijo confirmó igualdad exacta de los CSV de velocidad
y tinta y de los PNG sin HUD en las cinco resoluciones con suavizado; también
a 40 × 25 y 128 × 80 sin él. Es validación de equivalencia, no una nueva medición
de aceleración. Una prueba con ventana ejercitó los controles conservados,
las teclas retiradas, los parámetros fijos y el nuevo HUD. WSL completó además
65 pasos automáticos a 128 × 80. Evidencia: `target/ink-simplify-20261003/`.
SHA-256 de la muestra simplificada:
`92bdee2b991fd15bb2a63eb8da3d0a3c5b54566e8887c46c94e27e67619a578d`.

### Optimización de la simulación

El original proyectaba la velocidad dos veces por paso, con ocho barridos
Gauss-Seidel por proyección. El ejemplo final primero transporta la velocidad
corregida del paso anterior, después añade fuentes y vorticidad, realiza **una
proyección SOR con cuatro barridos y ω = 1,5**, y finalmente transporta la tinta.
También proyecta la rotación inicial al arrancar y después de los reinicios Q/R.

La actualización de presión es
`P(I)=0.375*(DIV(I)+P(I-1)+P(I+1)+P(I-S)+P(I+S))-0.5*P(I)`.
Los bucles internos evitan cambios innecesarios de línea y cálculo repetido de
índices. La presentación sustituye los productos matriciales densos por cálculo
directo de las mismas densidades bilineales antes de aplicar la curva de color.

La variante exploratoria que conservaba íntegramente el solver original sólo
aplicaba transformaciones equivalentes. **La variante final cambia la evolución
del fluido** por el orden de integración y la aproximación de presión; sus
trayectorias, campos y PNG no son idénticos al original. Los hashes distintos
son esperados. Estas mediciones no demuestran que la diferencia sea imperceptible
en cualquier interacción ni que ambos métodos tengan el mismo error numérico.

## Método

- Windows 11, mismo ejecutable y máquina para referencia y candidato; mediciones
  sin ventana y con ventana en tandas separadas.
- Escenario automático, suavizado y vorticidad activados, HUD desactivado,
  sin límite de FPS y paso simulado fijo `DT=1/30`.
- Un par de calentamiento externo y cuatro pares medidos, alternando AB/BA.
  Cada proceso calcula cinco fotogramas internos de calentamiento y 90 medidos.
- Los FPS proceden de `TIME` alrededor del bucle, sin temporizadores por etapa.
  El PNG y el CSV se exportan después. Se guarda también el tiempo monotónico
  de proceso, que incluye arranque, calentamiento y exportación.
- Se verifican terminación, marcadores, duración positiva, PNG 640 × 480 y
  campos finitos. Cada variante reproduce exactamente sus hashes de PNG, CSV
  y stdout sin la línea de tiempo en todos sus procesos, incluidos calentamientos.
  El CSV incluye bordes y usa la precisión de serialización de BASIC `WRITE`;
  no es una comparación de los bits internos de cada número.

Las tablas muestran **mediana ± MAD** de FPS; MAD es la desviación absoluta
mediana, no un intervalo de confianza. Se mide capacidad del bucle sin límite,
no la frecuencia física de refresco del monitor. Son cargas automáticas finitas,
no una garantía de FPS constante durante toda sesión interactiva.

## Windows con ventana

| Cuadrícula | Original, FPS | Final, FPS | Aceleración |
| --- | ---: | ---: | ---: |
| 64 × 40 | 63,364 ± 0,027 | 122,903 ± 0,105 | 1,94× |
| 96 × 60 | 27,922 ± 0,085 | 55,848 ± 0,260 | 2,00× |
| 128 × 80 | 15,022 ± 0,005 | 31,897 ± 0,017 | 2,12× |

## Windows sin ventana

La rasterización sigue activa; sólo se desactiva la ventana.

| Cuadrícula | Original, FPS | Final, FPS | Aceleración |
| --- | ---: | ---: | ---: |
| 40 × 25 | 164,01 ± 0,43 | 315,72 ± 0,85 | 1,93× |
| 64 × 40 | 63,94 ± 0,53 | 125,58 ± 0,73 | 1,96× |
| 80 × 50 | 40,83 ± 0,12 | 82,02 ± 0,04 | 2,01× |
| 96 × 60 | 28,23 ± 0,14 | 56,62 ± 0,11 | 2,01× |
| 128 × 80 | 15,19 ± 0,02 | 32,34 ± 0,10 | 2,13× |

## Pincel, controles y WSL

El escenario de pincel determinista a 128 × 80, con ventana, también mejora:
**15,148 ± 0,029 → 32,189 ± 0,005 FPS** (2,13×). Son dos pares AB/BA de
180 fotogramas medidos, con cinco internos de calentamiento y sin calentamiento
externo; esta tanda complementaria tiene menos repeticiones que la automática.
El pincel recorre una curva, alterna tinta, agitación y liberación, y cambia color.

Las tres tandas Windows suman **84 procesos**: 50 automáticos sin ventana,
30 automáticos con ventana y cuatro de pincel. Se comprobaron **486.780 registros
de celdas**, todos finitos y con tinta dentro de [0,16]; el máximo observado fue
8,3960. El determinismo de cada variante se mantuvo en las tres tandas.

Una secuencia adicional ejercitó Q en los cinco tamaños, B, V, C, R, pausa,
colores 1–5, emisores y pincel; P recorrió 4 → 8 → 16 → 32 → 4. Su repetición
produjo los mismos campos, imagen final y tres capturas intermedias. Es una
comprobación programada de esas rutas, no una medición de latencia del ratón.

En WSL/Linux terminó correctamente una comprobación funcional del candidato
con pincel a 128 × 80, sin ventana, de **185 fotogramas totales**. Su PNG final
coincide con el del candidato Windows para ese escenario. No se presenta como
una comparación de rendimiento entre original y candidato en Linux ni como
igualdad de todos los valores internos entre plataformas.

La evidencia local adicional está en `final-manual/report.json`,
`final-validation.json`, `fast-controls-check.json` y `wsl-final/validation.json`,
bajo `target/ink-optimization-20261003/`.

## Identidad y reproducción

| Elemento medido | SHA-256 |
| --- | --- |
| Fuente original | `f39b830fb6bf5749d575afc597b02267cb8a3bcc65ed7d2f684d05fdd434eaaf` |
| Fuente final | `562028938473ffd8691692db16dabd4934495a0bdddd77af31eaa8f8c38612b2` |
| Ejecutable Windows | `c6ff712fad985071490ece9a1d4d151b857096475ae239c0aad7a4c566192da5` |

Los resultados completos están localmente en
`target/ink-optimization-20261003/final-headless/report.json` y
`target/ink-optimization-20261003/final-window/report.json`. Incluyen tiempos
individuales, hashes de cada programa adaptado y de sus salidas. La referencia
original se conserva **sólo en la copia local**
`target/ink-optimization-20261003/baseline.bas`; estos directorios no se presentan
como evidencia versionada. Los informes de ambas tandas tienen estado `passed`.

Desde la raíz del repositorio Rust, con esa referencia disponible y directorios
de salida nuevos:

```powershell
python tools/benchmarks/ink_reactor.py --baseline target/ink-optimization-20261003/baseline.bas --output target/ink-recheck-headless --grids 40,64,80,96,128 --frames 90 --runs 4 --warmups 1 --window 0 --smooth 1 --scenario auto --allow-differences
python tools/benchmarks/ink_reactor.py --baseline target/ink-optimization-20261003/baseline.bas --output target/ink-recheck-window --grids 64,96,128 --frames 90 --runs 4 --warmups 1 --window 1 --smooth 1 --scenario auto --allow-differences
```

`--allow-differences` permite diferencias entre versiones, pero exige determinismo
dentro de cada una. El harness conserva el número de iteraciones inicial de cada
fuente: ocho en la referencia y cuatro en el candidato. Para repetir la tanda de
pincel, usar `--grids 128 --frames 180 --runs 2 --warmups 0 --window 1 --scenario manual`
con otro directorio nuevo y las mismas opciones de referencia y diferencias.
