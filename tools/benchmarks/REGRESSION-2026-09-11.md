# Regresión de rendimiento corregida en Rust — 11 de septiembre de 2026

La comparación de los paquetes publicados sitúa la regresión en `e17f163`
(versión 1.5.94, conservación de líneas durante MERGE). Entre 1.5.93 y 1.5.94,
Pi de 10.000 decimales pasa de 1.5630 a
1.8738 s y Jelly, de
0.9106 a 1.0069 s
para 320 fotogramas sin ventana. Estos valores son de esta máquina; no son
promesas universales ni se mezclan con los controles de la corrección.

La corrección se aplica sobre 1.5.95 con los cambios locales de consola que ya
existían al comenzar. El control final se reconstruyó con exactamente esa fuente
sin el parche de rendimiento. Python AVL-BASIC no se modificó ni ejecutó.

## Causa y cambio

`run_from_inner` clonaba y destruía una referencia contada a las instrucciones
de cada línea, incluso sin MERGE. Además, al volver del NEXT al punto inmediatamente
posterior a un FOR que termina su línea, buscaba la siguiente línea por número.
Ese trabajo se repetía en todos los bucles internos de Pi y en cada punto de Jelly.

En Windows, ahora las instrucciones se toman prestadas del catálogo local, que mantiene su
propiedad durante la línea. La recarga se difiere hasta la siguiente entrada al
bucle exterior y se marca al modificar el catálogo, reiniciar el programa o
despachar desde el sondeo de eventos. Así no se añade una comparación del catálogo
por cada línea. Para la línea ordinaria, avanzar al sucesor vuelve a ser sumar uno.

Los planes retenidos siguen usando la búsqueda por número: una línea eliminada
por MERGE puede tener ya como índice el de su sucesor. Se mantienen los ajustes
de cursores y errores, CHAIN, GOSUB/RETURN, STOP/CONT, RESUME y el depurador.
Durante la cola de una línea que hace MERGE se conserva el catálogo anterior
completo hasta abandonar esa línea; después se libera.

La estrategia se selecciona al compilar: Windows usa este camino y Linux conserva
el anterior. La variante compartida se rechazó en Linux tras una comparación nativa
con ambos ejecutables y archivos en ext4 y afinidad a CPU 0: empeoraba Jelly un
5,94 % en tiempo y el agregado un 1,65 %. No se atribuye esta diferencia a un
mecanismo concreto del compilador sin evidencia adicional. No hay una decisión
de plataforma por instrucción en el ejecutable.

No se modifican el algoritmo BASIC, los ejemplos ni las opciones del compilador.
El resultado medido corresponde al conjunto del parche; las mediciones no
asignan un porcentaje independiente a cada operación eliminada.

## Windows: ejecutable final

Tiempo externo monotónico, mediana de cuatro ejecuciones por ejecutable, más
un calentamiento; orden A/B alternado y cargas fijas de `representative.json`.
Sin compilaciones ni otros benchmarks simultáneos. Se comparan salidas y PNG
completos, ignorando únicamente el marcador de tiempo.

Tras seleccionar la estrategia por plataforma se recompiló Windows: la sección
`.text` es idéntica byte a byte a la del ejecutable medido en la tabla, ventana y
Pi100000. Se repitieron además Pi10000 y Jelly con el archivo final. Se guardan
los hashes de ambos ejecutables y la comparación de secciones PE.

| Programa | Antes (s) | Corregido (s) | Reducción del tiempo |
|---|---:|---:|---:|
| raytracer | 2.3221 | 2.1794 | +6.15 % |
| pimachin | 1.8430 | 1.6398 | +11.02 % |
| nqueens | 1.5092 | 1.3818 | +8.44 % |
| jelly | 0.9614 | 0.9263 | +3.65 % |
| mandelbrot | 0.6569 | 0.5896 | +10.24 % |
| strings | 1.3309 | 1.3302 | +0.05 % |
| modern | 2.1103 | 1.9380 | +8.16 % |
| matrices | 0.9534 | 0.9482 | +0.54 % |

Jelly con ventana real: cuatro ejecuciones de 1.600 fotogramas, más un
calentamiento. La media de fotogramas por segundo calculada a partir de la
mediana del tiempo BASIC pasa de **316.4 a 329.9 FPS**.
Es la media de la ejecución finita, no una lectura del indicador instantáneo.
Se comprobó que el reloj BASIC no contradice el tiempo externo en estas muestras.

Pi de **100.000 decimales**: **180.999 →
160.224 s**, reducción del **11.48 %**.
Es una ejecución completa por ejecutable, sin calentamiento; tiene menos
repetición estadística que las comparaciones cortas y no es una extrapolación.

## WSL/Linux

Compilación release nativa con Rust 1.95.0, conservando la estrategia anterior.
Control y candidato, programas y resultados en el sistema de archivos Linux;
afinidad a CPU 0, cuatro ejecuciones por programa y un calentamiento, sin ventana.
Los porcentajes se calculan dentro de cada plataforma. Las oscilaciones pequeñas
de esta comparación no se presentan como una optimización de Linux.
Se observaron saltos del marcador BASIC `TIME` en algunas ejecuciones WSL;
por eso la tabla usa exclusivamente el reloj externo monotónico.

| Programa | Antes (s) | Corregido (s) | Reducción del tiempo |
|---|---:|---:|---:|
| raytracer | 2.1928 | 2.2080 | -0.69 % |
| pimachin | 2.1525 | 2.1504 | +0.10 % |
| nqueens | 1.3919 | 1.3847 | +0.52 % |
| jelly | 0.9293 | 0.9352 | -0.63 % |
| mandelbrot | 0.6030 | 0.6068 | -0.63 % |
| strings | 0.7714 | 0.7735 | -0.27 % |
| modern | 2.4602 | 2.4564 | +0.16 % |
| matrices | 0.4767 | 0.4744 | +0.49 % |

## Validación

- Windows: 639 pruebas superadas, cero fallos;
  seis pruebas que requieren interacción gráfica permanecen ignoradas.
- WSL: 639 pruebas superadas, cero fallos.
- En cada plataforma se filtraron 25 pruebas
  por los nombres `python` y `corpus_matches_python_checkout`; no se ejecutó el
  intérprete Python. El corpus autónomo Rust de CHAIN/MERGE sí se ejecutó.
- Nueva regresión: STOP al final de una línea borrada por MERGE, seguido de CONT,
  ejecuta el sucesor insertado tanto con como sin depurador.
- Salidas y PNG coinciden entre control y candidato en todas las cargas medidas.
- Pi se contrastó además con Chudnovsky por división binaria y enteros exactos,
  con cotas coincidentes de truncamiento. Los primeros 10.000 y 100.000 decimales
  solicitados son correctos. El ejemplo imprime ocho decimales adicionales;
  esa cola aproximada se registra aparte y no se cuenta como precisión solicitada.
- Las ediciones previas de consola y sus pruebas se conservaron; se guardó el
  diff inicial para distinguirlas del parche de rendimiento.

## Archivos y reproducción

- Ejecutable Windows de desarrollo: `target/release/avl-basic.exe`.
- Copias finales: `target/perf-regression-20260911/final-portable-windows.exe`
  y `target/perf-regression-20260911/final-portable-linux`.
- Evidencia compacta y hashes: [regression-results-2026-09-11.json](regression-results-2026-09-11.json).
- Resultados crudos, controles, fuente congelada, logs, programas generados,
  verificador y oráculo: `target/perf-regression-20260911/`.
- Parche aislado: `target/perf-regression-20260911/performance-fix.patch`.

Desde la raíz Rust, para repetir la comparación en un directorio nuevo:

```powershell
python tools/benchmarks/representative.py --baseline target/perf-regression-20260911/current-source-control.exe --candidate target/release/avl-basic.exe --runs 4 --warmups 1 --output target/perf-regression-20260911/repeat-new
```

Las mediciones de este estudio corresponden a compilaciones de la versión
1.5.95 con el arreglo local. La corrección se incorpora a la versión 1.5.96;
los hashes anteriores identifican los binarios medidos, no los paquetes de esa
nueva versión.
