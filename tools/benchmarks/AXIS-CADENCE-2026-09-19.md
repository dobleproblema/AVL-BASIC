# Comprobación tras unificar la cadencia de marcas y etiquetas

Comparación breve del ejecutable con marcas principales y etiquetas seleccionadas juntas, margen de 10 píxeles y subdivisiones entre las marcas finales, frente al ejecutable de ejes automáticos inmediatamente anterior.

| Carga | Antes, mediana (s) | Después, mediana (s) | Cambio de tiempo |
| --- | ---: | ---: | ---: |
| Pi, 10.000 decimales | 1.727838 | 1.734803 | +0.40 % |
| Jelly, sin ventana, 10.000 puntos y 320 fotogramas | 0.963720 | 0.956146 | -0.79 % |

Pi tarda algo más en las cuatro parejas; Jelly tarda menos en las cuatro. Se conservan los dos resultados sin atribuirlos a una causa física concreta. Esta comparación no vuelve a medir contra la referencia anterior a los ejes automáticos ni resuelve la diferencia pendiente descrita en [AXIS-AUTO-2026-09-19.md](AXIS-AUTO-2026-09-19.md).

Método: `representative.py`, afinidad LP2 mediante `compare-lp2.py`, un calentamiento y cuatro ejecuciones medidas por binario/carga, orden AB/BA equilibrado, tiempo externo monotónico. Sin compilaciones ni pruebas simultáneas. Las salidas normalizadas y la imagen final de Jelly coinciden. No son medidas de FPS con ventana ni de rendimiento Linux.

Referencia Windows: `af8b7c07ed35565eabdf3e76964e68851be43a7de1a66a8ba4379d26e86f68fe`. Ejecutable actualizado: `83d2910c4bf4ee42191201d87895ac6fa4f7f8f2e2248a942037850c66f50460`.

Datos completos: [axis-cadence-results-2026-09-19.json](axis-cadence-results-2026-09-19.json). Programas y salidas crudas: `target/axis-cadence-20260919/performance-windows`.

## Seguimiento: etiquetas próximas al eje perpendicular

La corrección posterior omite las etiquetas cuya caja invade el eje perpendicular, sin alterar la selección regular de marcas. Con el mismo método, comparando contra `83d2910c4bf4ee42191201d87895ac6fa4f7f8f2e2248a942037850c66f50460`, el ejecutable `5863ff05e538f29fa6b977db03b9f89757efc4d1988c764a72c169e555214b84` obtiene:

| Carga | Antes, mediana (s) | Después, mediana (s) | Cambio de tiempo |
| --- | ---: | ---: | ---: |
| Pi, 10.000 decimales | 1.744391 | 1.752322 | +0.45 % |
| Jelly, sin ventana | 0.956730 | 0.960570 | +0.40 % |

Pi tarda más en las cuatro parejas; Jelly en tres de cuatro. Las salidas y la imagen final siguen coincidiendo. Se registra esta comparación breve sin atribuir una causa ni extrapolar a FPS con ventana. Datos: [axis-crossing-results-2026-09-19.json](axis-crossing-results-2026-09-19.json); programas y salidas crudas en `target/axis-crossing-20260919/performance-windows`.

## Seguimiento: selección anclada al valor del cruce

Ambas etapas de reducción conservan ahora el valor de `CROSSAT`, en lugar de priorizar el cero. Con el mismo método, frente a `5863ff05e538f29fa6b977db03b9f89757efc4d1988c764a72c169e555214b84`, el ejecutable `f51ab058dcb0ad0c0cc8ed6e214d2e932398d2c3c1ab1cdb432ba93688d13e5a` obtiene:

| Carga | Antes, mediana (s) | Después, mediana (s) | Cambio de tiempo |
| --- | ---: | ---: | ---: |
| Pi, 10.000 decimales | 1.745830 | 1.749876 | +0.23 % |
| Jelly, sin ventana | 0.954184 | 0.967351 | +1.38 % |

Ambas cargas tardan más en las cuatro parejas. Las salidas y la imagen final coinciden con la referencia. La diferencia queda registrada y pendiente de diagnóstico; no se atribuye a una causa física ni se extrapola a FPS con ventana. Datos: [axis-anchor-results-2026-09-19.json](axis-anchor-results-2026-09-19.json); programas y salidas crudas en `target/axis-anchor-20260919/performance-windows`.
