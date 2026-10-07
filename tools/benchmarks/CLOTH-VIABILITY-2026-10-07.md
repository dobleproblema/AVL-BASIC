# g-cloth: viabilidad de 10 FPS

Estudio local, 7 de octubre de 2026. Referencia: Rust AVL-BASIC 1.6.11,
commit `d78da8ddfcd9026a143ef54cfb5b57aa3db1fbb6`.

Estado posterior al estudio: por decisión del usuario, g-cloth se ha retirado
del catálogo y sus archivos se han archivado en
`target/cloth-perf/study-artifacts/retired-demo`. El candidato de expresiones
queda archivado, sin integrarse. GTRIANGLE se incorpora por separado en Rust y
Python, con coordenadas ORIGIN/SCALE, INK conservado y cursor en el tercer
vértice. Las mediciones siguientes corresponden a los binarios experimentales
indicados, cuyo contrato era distinto; no son cifras del nuevo intérprete.

**No hay una vía acotada y demostrada para alcanzar 10 FPS conservando esta
calidad.** Los prototipos pasan de **0,90 a 1,25 FPS con ventana real**, con
física e imagen idénticas en las comparaciones. Todavía haría falta acelerar el
fotograma completo unas **8 veces**. El principal obstáculo es la física BASIC.

Esto no demuestra que un BASIC con compilación nativa de bloques no pueda
lograrlo. Esa posibilidad supondría un proyecto de compilador mucho mayor que
incorporar un triángulo y mejorar algunas expresiones; su rendimiento no se ha
medido aquí.

## Qué se ha probado

Los cambios de ejecución se han hecho en un checkout experimental aislado:
`target/cloth-perf/study`. Durante el ensayo, el intérprete principal, su
ejecutable y `samples/g-cloth.bas` conservaron sus contenidos anteriores.

1. **GTRIANGLE genérico:** triángulo definido por coordenadas de pantalla,
   profundidad inversa y RGB por vértice. Interpolación de color con corrección
   de perspectiva y comprobación de profundidad por píxel. La demo sigue
   calculando geometría, cámara, luces, material, sombras y física en BASIC.
   Se sustituye únicamente su rasterizador, líneas 7000–7999.
2. **Optimización interna de expresiones existentes:** planes numéricos para
   ciertas asignaciones a escalares y arrays de una dimensión, con al menos
   dos lecturas de array, índices simples y operaciones `+`, `-`, `*`, `/`,
   `SQR`. Conserva orden de evaluación, errores, aliases y fallback cuando
   no se cumplen las condiciones. No añade sintaxis al BASIC.
3. **Diagnóstico de FOR/NEXT y GOSUB/RETURN:** lectura del control compilado
   existente y medición de programas mínimos con un millón de iteraciones.

No se usan primitivas de telas, restricciones, colisiones o motores 3D.
El GTRIANGLE medido era un prototipo pendiente de diseño final de API,
documentación e integración; en aquel ensayo todavía no era una función del
intérprete principal. Su integración posterior se describe al principio de este
informe; las cifras siguientes conservan su carácter histórico.

## Calidad y método

Se congela la demo original: imagen de **800×600**, malla **40×34** con
**1435 nodos**, seis subpasos `H=1/180`, doce iteraciones XPBD y doce pasadas
adicionales de límites y contactos. Se mantienen autocontacto, viento, material,
sombras y todas las ecuaciones. No se reduce detalle ni precisión física.

Cada proceso carga el mismo checkpoint DRAPE obtenido tras 600 subpasos de la
demo original. Después ejecuta seis fotogramas completos, con física y dibujo
intercalados: descarta el primero y mide los cinco siguientes. Se excluyen
inicialización, carga del checkpoint y exportaciones. El tiempo se mide desde
fuera del intérprete con `perf_counter` y marcas de stdout; no depende de la
resolución de `TIME`.

Por comparación se ejecuta una pareja de calentamiento y cuatro parejas medidas
con orden AB/BA. No hay builds ni benchmarks simultáneos. Se informa de la
mediana de las medianas de cada proceso y de su MAD. FPS = inverso del tiempo
TOTAL. Las etapas tienen medianas independientes y no deben sumarse como si
fueran un mismo fotograma.

Son **50 procesos de cloth**, 40 medidos y 10 de calentamiento, con series
separadas sin ventana y con ventana real. Equipo registrado: Windows 11 x64,
Intel Family 6 Model 183 Stepping 1, 24 procesadores lógicos.

## Resultados

### Ventana real

| Variante | Total ms/frame | MAD ms | FPS | Física ms | Dibujo ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Original | 1114,10 | 1,19 | 0,898 | 717,83 | 392,69 |
| GTRIANGLE | 827,90 | 6,15 | 1,208 | 731,41 | 97,04 |
| GTRIANGLE + expresiones genéricas | 800,44 | 1,68 | 1,249 | 704,86 | 95,28 |

GTRIANGLE reduce el tiempo total un **25,05%** en su bloque de comparación.
Las expresiones añaden un **2,81%** de reducción en su propio bloque. Las
medianas de reducciones emparejadas son 25,13% y 2,84%, respectivamente.

### Sin ventana

| Variante | Total ms/frame | MAD ms | FPS | Física ms | Dibujo ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Ejecutable original | 1119,81 | 1,96 | 0,893 | 725,27 | 393,76 |
| Control recompilado, raster BASIC | 1129,89 | 2,93 | 0,885 | 737,95 | 390,21 |
| GTRIANGLE | 832,28 | 1,91 | 1,202 | 733,83 | 97,84 |
| GTRIANGLE + expresiones genéricas | 813,80 | 0,34 | 1,229 | 716,13 | 97,35 |

El control recompilado permite medir el cambio de raster con el mismo binario:
GTRIANGLE reduce el tiempo un **26,19%**. Las expresiones añaden **2,22%**.
La recompilación del control cambia el tiempo original aproximadamente un 1%;
no se atribuye ese cambio a una causa de CPU o compilador demostrada.

GTRIANGLE concentra el ahorro esperado: la etapa que rasteriza la escena pasa
de unos **332 a 36 ms**. Sin embargo, quedan alrededor de **47 ms de sombreado
BASIC**, **9 ms de sombras** y el trabajo de preparación y dibujo restante.
El render completo todavía consume casi todo el presupuesto de 100 ms.

## Qué falta para 10 FPS

Con el mejor prototipo de ventana, la física sola tarda **704,86 ms**.
Incluso un render gratuito dejaría el máximo en aproximadamente **1,42 FPS**
si la física permaneciera igual.

Presupuesto orientativo: `aceleración física = 704,86 / (100 - dibujo_ms)`.
Se supone despreciable cualquier otro coste; no es una predicción de rendimiento.

| Dibujo restante supuesto | Presupuesto para física | Aceleración física necesaria |
| --- | ---: | ---: |
| 0 ms | 100 ms | 7,05× |
| 10 ms | 90 ms | 7,83× |
| 20 ms | 80 ms | 8,81× |
| 40 ms | 60 ms | 11,75× |
| 60 ms | 40 ms | 17,62× |

Por tanto, otra operación gráfica por lotes podría aportar una mejora útil,
pero no resolvería por sí sola el coste físico. Las restricciones actualizan
posiciones compartidas secuencialmente: convertirlas directamente en operaciones
vectoriales independientes cambiaría el algoritmo y requiere otra validación.

El perfil físico previo, medido por separado sobre el mismo checkpoint y
ejecutable original, sitúa el tiempo en restricciones de distancia (~215 ms),
límites de extensión (~178 ms), flexión (~145 ms), autocontacto (~116 ms),
contactos con esfera/suelo (~46 ms) y viento (~19 ms). Es un diagnóstico de
prioridades; la serie de fotogramas completos es la medida principal.

## Bucles, expresiones y arrays

La física DRAPE ejecuta por fotograma **2.350.290 NEXT**, **224.359 entradas
FOR** y **16.938 GOSUB**. No incluye WHILE de longitud variable ni el render.

NEXT ya incrementa un slot numérico y compara límites guardados; no analiza
otra vez la instrucción ni busca la variable en una tabla de nombres en cada
vuelta. GOSUB literal ya conserva su destino compilado. El trabajo restante
incluye evaluación de expresiones, acceso y escritura de arrays, condiciones,
comprobaciones y despacho de las instrucciones del cuerpo.

Diagnóstico con el ejecutable original, cuatro parejas AB/BA y calentamiento
por proceso:

| Programa mínimo | Trabajo medido | Mediana ms | MAD ms |
| --- | --- | ---: | ---: |
| NEXT I | Un millón de vueltas | 12,08 | 0,03 |
| NEXT sin nombre | Un millón de vueltas | 9,92 | 0,11 |
| GOSUB/RETURN vacío | Un millón de vueltas con una llamada por vuelta | 30,40 | 0,12 |
| Bucles cortos anidados | Un millón de entradas internas; cuatro millones de NEXT | 65,37 | 0,41 |

Estos programas miden una referencia de control y despacho, no su coste exacto
dentro de cloth. No permiten asignar un porcentaje preciso de los 705 ms a
NEXT. Sí desaconsejan esperar cientos de milisegundos de ahorro por quitar
nombres a NEXT o por retoques similares.

La optimización genérica del RHS demuestra que queda cierto margen, pero el
2–3% observado está muy lejos del factor requerido. Tampoco es una mejora
universal: en cuatro comparaciones AB/BA de otros programas, el tiempo de proceso
cambia **−1,37% en Pi10000**, **+1,30% en Jelly sin ventana** y **+0,33% en
matrices**, con salidas y PNG coincidentes. No se recomienda incorporar este
candidato tal cual por el beneficio de cloth. Este estudio no resuelve la
regresión histórica de rendimiento de Rust.

Compilar bloques o bucles completos podría eliminar mucho más trabajo sin
introducir conocimientos de física en el lenguaje. Habría que preservar aliases,
errores y líneas precisas, interrupciones, cambios del programa y ejecución
alternativa cuando fallen las condiciones de compilación. Es una posibilidad
técnicamente coherente, pero aquí no hay un prototipo ni mediciones para prometer
que alcance 10 FPS. Un intérprete de bytecode también podría reducir despacho;
no se puede equiparar ese ahorro con ejecutar todo el cuerpo como código nativo.

## Comprobación del resultado y límites

- Los 50 procesos de cloth producen CSV, píxeles y PNG idénticos entre variantes:
  posiciones, velocidades, contactos, restricciones y estado global exportado.
- GTRIANGLE reproduce la imagen completa y todos los valores de profundidad
  exportados en **640×480 y 800×600**, además del cursor e INK. Ocho pruebas de
  escena pequeñas cubren orden de vértices, perspectiva, cruces de profundidad,
  recorte, colores, empates y casos degenerados. No se afirma identidad binaria
  universal de coma flotante a partir de las exportaciones decimales.
- Windows: suite completa del candidato, **980 tests aprobados, 8 ignorados**;
  también pasó la suite del control. Linux/WSL: builds release de ambos prototipos
  y **536 tests aprobados** de biblioteca, expresiones y triángulos.
- Los FPS son una medición local de una secuencia finita DRAPE. No se ha medido
  una sesión larga, HANG, todos los materiales ni todos los gestos del ratón.
  El autocontacto tiene trabajo variable. Los resultados Linux son validación
  de compilación y comportamiento, no una medida de FPS Linux.
- El estudio conserva estrictamente las ecuaciones y parámetros. No evalúa
  reducir la malla, las iteraciones o el detalle: sus posibles ganancias
  necesitarían demostrar que conservan el carácter visual y físico de la demo.

## Decisión propuesta al terminar el estudio

Mantener g-cloth como experimento visual y considerar GTRIANGLE por su utilidad
general. **No plantear 10 FPS como objetivo alcanzable mediante unas cuantas
optimizaciones más de esta demo.** Para perseguirlo habría que decidir abrir un
proyecto de compilación general y validar su rendimiento antes de comprometer
el resultado. No hace falta añadir un motor de telas al intérprete para llegar
a esta conclusión.

## Evidencia y reproducción

Resumen durable con configuración, hashes, medianas, comparaciones y validación:
[cloth-viability-2026-10-07.json](cloth-viability-2026-10-07.json).
Runner: [cloth_viability.py](cloth_viability.py).

La evidencia completa permanece local en
`target/cloth-perf/study/target/results-headless`, `results-windowed`,
`results-loops`, `results-regression`, `gtri-equivalence` y `gtri-small`.
El parche experimental recuperable y auxiliares están en
`target/cloth-perf/study-artifacts`; los cuatro binarios Windows/Linux,
en `target/cloth-perf/bin`. Estos archivos bajo target no están versionados.

Identidades de los binarios Windows:

| Binario | SHA256 |
| --- | --- |
| Original | `9c38031804a68fa0eb3286eab3fcfbac1bbb715849ad8344ce297f363270185f` |
| GTRIANGLE, expresiones originales | `bee190fb3988efd5e3bcc3f130fb5a0188e22bf186f81d18be842fb4568ca1d2` |
| GTRIANGLE + expresiones genéricas | `5c9e70550cd99538b8a99c22396d7feb5e138aaffb6fbca387d215b69ebfa5c8` |

Fuente congelada: `429865fe58ebb3ce6e274cdfd455646c5af79a931cc5b72b3edfe42785072549`.
Checkpoint: `eb4494248de6bf8b6f3c826d530278154f4acadf1c2ab48fdfc429af360a6ad4`.

Desde el checkout Rust principal, repetir la serie de ventana en un directorio
nuevo; requiere conservar los archivos locales anteriores:

```powershell
python tools/benchmarks/cloth_viability.py --output target/cloth-perf/repeat-windowed --variant current target/release/avl-basic.exe target/cloth-perf/baseline.bas --variant gtri target/cloth-perf/bin/gtri-win.exe target/cloth-perf/study-artifacts/g-cloth-gtri.bas --variant numeric target/cloth-perf/bin/numeric-win.exe target/cloth-perf/study-artifacts/g-cloth-gtri.bas --compare current gtri --compare gtri numeric --window 1 --pairs 4 --warmup-pairs 1
```

Para la serie sin ventana se añade la variante `rebuilt` (gtri-win.exe con
baseline.bas), se compara `current→rebuilt→gtri→numeric` y se usa `--window 0`.
El parche parte del commit indicado y se conserva sin aplicarlo al checkout
principal. `AVL_STUDY_GENERIC_RHS=0` y `=1` se seleccionan al compilar en release;
no son ajustes dinámicos de la demo. Se utilizaron rustc 1.99.0 en Windows y WSL,
`opt-level=3`, LTO thin y una unidad de generación de código.
