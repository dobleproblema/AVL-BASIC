# Disposición del código y regresión de 1.6.6 — 25 de septiembre de 2026

Se reproduce una regresión entre los publicados 1.6.5 y 1.6.6: **+10,91 % de
tiempo en Pi de 10.000 decimales y +14,98 % en Jelly sin ventana**. En las rutas
examinadas no hay instrucciones nuevas ni comprobaciones adicionales durante
estos programas. Cambia la ubicación del código máquina dentro del ejecutable.

La corrección separa el bucle ordinario y el cuerpo recursivo del evaluador
numérico en dos secciones ejecutables independientes. Recupera buena parte del
rendimiento y reduce la sensibilidad al cambio concreto que produjo esta
regresión. **La recuperación es parcial: no se dan por recuperados los 330 FPS
históricos en todas las condiciones, ni por identificado el mecanismo físico
exacto de la CPU.**

## Evidencia de la causa

El último cambio funcional valida la dimensión de `LBOUND`/`UBOUND` y elimina
`LBND`/`UBND`. Pi y Jelly no llaman esas funciones. No se endureció el acceso
ordinario a los elementos de arrays en esta versión.

La comparación de los PE publicados y del PDB actual encuentra:

- `run_from_inner`: 20.035 bytes y 3.753 instrucciones en ambos binarios.
- 55 destinos directos con límites verificables: mismos tamaños, instrucciones,
  desplazamientos internos y operandos tras normalizar direcciones de imagen.
  Hay otros dos destinos sin rango `.pdata` delimitable; no se extiende la
  afirmación de equivalencia a todo el grafo de llamadas.
- Cuerpo recursivo de `FastNumberExpr::eval`: 6.155 bytes, 1.317 instrucciones
  y 12 llamadas recursivas. Su posición módulo 64 cambia de 32 a 48.
- El bucle y muchos auxiliares se desplazan -1.008 bytes. La sección `.text`
  se reduce 1.104 bytes; no hay evidencia de un límite de tamaño alcanzado.

Los nombres privados se infieren mediante los llamadores, la recursión y los
rangos `.pdata`; no todos están disponibles como símbolos privados en el PDB.
La identidad del PDB actual se verifica por GUID y Age, sin usar un PDB de otra
compilación. El informe estático y los inventarios detallados se conservan en
`target/perf-regression-20260925/binary-audit/`.

Una ablación que restaura sólo el cuerpo antiguo de la función de límites,
sin invocarla desde las cargas, reduce Pi un 4,89 % y Jelly un 8,17 % frente al
control. Son dos parejas más calentamiento y no una corrección para entregar:
esa variante revierte comportamiento solicitado. Refuerza la relación con la
disposición del binario, no con ejecutar las nuevas validaciones.

## Corrección y alcance

Dos atributos `link_section`, limitados a Windows x64 MSVC, sitúan las funciones
en `.avlrun` y `.avleval`. No se cambia el evaluador, la representación del
intérprete, las opciones del perfil release ni las reglas de BASIC. Se conserva
el inlining existente. Las copias insertadas por el compilador siguen en sus
llamadores; se comprueba la ubicación del cuerpo recursivo realmente emitido.

En el ejecutable final ambas secciones empiezan en límites de página de 4 KiB:
RVA `0x25c000` y `0x261000`. Sus permisos son código/lectura/ejecución
(`0x60000020`), sin escritura. Conservan sus rangos `.pdata` y registros de
desenrollado. Las instrucciones de ambos cuerpos y de los 55 destinos
delimitables se conservan frente al control, normalizando las direcciones.

Aplicar los mismos atributos a la variante con el código y catálogo de 1.6.5
y a la de 1.6.6 reduce el salto observado: Pi queda un 2,14 % más rápido y Jelly
un 1,20 % más lento en la variante nueva. Se reutiliza el cambio funcional real
como perturbación, sin buscar rellenos ni reordenamientos arbitrarios del código.
Esto estabiliza las entradas examinadas, no todas las decisiones futuras del
compilador, los auxiliares ni el comportamiento de cachés y predictores.

Se probaron también secciones para sólo el evaluador y sólo el bucle. El caso
de sólo el bucle dio mejor Jelly en su piloto, pero deja la entrada recursiva
en `.text`, expuesta a los desplazamientos originales. Se conserva la separación
de ambos cuerpos, cuya estabilidad se verificó con la pareja funcional anterior.

La última prueba añadió una salida anticipada al sondeo cuando no hay audio,
temporizadores ni handlers. Frente al candidato con secciones dio +0,31 % de
reducción en Pi y -0,99 % en Jelly; no aporta una mejora suficiente y se descartó.
El código de audio no forma parte del arreglo final.

## Medición del ejecutable final

Misma máquina i7-13700K, Windows x64, mismo núcleo P (procesador lógico 2,
afinidad 0x4), Rust 1.95.0 / LLVM 22.1.2, perfil release habitual. Cuatro
repeticiones medidas y un calentamiento por binario/carga; orden AB/BA,
rotación de cargas, sin compilaciones, pruebas ni otros benchmarks simultáneos.
Los tamaños de `representative.json` se mantuvieron fijos. Se usa la mediana
del tiempo externo del proceso; los JSON incluyen dispersión y cada ejecución.

| Carga | Publicado 1.6.6 (s) | Corregido (s) | Reducción del tiempo |
| --- | ---: | ---: | ---: |
| Pi 10.000 | 1,918007 | 1,766472 | 7,90 % |
| Jelly sin ventana, 320 frames | 1,099824 | 0,997466 | 9,31 % |
| Raytracer | 2,364597 | 2,267944 | 4,09 % |
| N-Queens | 1,459525 | 1,388800 | 4,85 % |
| Mandelbrot | 0,717435 | 0,644620 | 10,15 % |
| Texto | 1,374745 | 1,372413 | 0,17 % |
| Pi moderno | 2,224741 | 2,090853 | 6,02 % |
| Matrices | 0,982705 | 0,987211 | -0,46 % |

La reducción geométrica agregada es 5,32 %. Texto y matrices cambian poco;
sus pequeñas diferencias cambian de signo entre la ronda provisional y la
final. No se presenta esa media como ganancia universal.

Jelly con ventana real, 10.000 puntos y 1.600 frames, pasa de **283,50 a
315,00 FPS** (+11,11 %). El tiempo externo pasa de 5,669144 a 5,101651 s
(-10,01 %). Los FPS se calculan con el tiempo del cuerpo BASIC; todos los
marcadores de esta ronda son positivos y menores que el tiempo externo.
No se mezclan esta carga y la medición sin ventana.

Las dos cargas adicionales de verificación dan 6,98 % de reducción en `7^10000`
y 0,45 % en edición Unicode. Sus oráculos independientes verifican el entero
completo y los textos/checksums de las 20 ejecuciones. Estos casos ya se habían
consultado en investigaciones anteriores: no son cargas inéditas de validación.

## Verificación y artefactos

- 765 pruebas Rust pasadas en Windows y 775 en WSL; 7 y 1 ignoradas. Las
  integraciones opcionales con el checkout Python no se activaron en estas
  suites. Los oráculos matemático y de Unicode sí se ejecutaron expresamente.
- Todas las comparaciones conservan salida/error normalizados y el PNG final
  de los casos gráficos. Esto no verifica todos los fotogramas intermedios.
- El código ejecutable del final Windows (`.text`, `.avlrun`, `.avleval`) es
  idéntico byte a byte al candidato provisional, con las mismas direcciones.
  También coinciden `.pdata` y los registros de desenrollado. Hay diferencias
  en cabeceras y `.rdata`; no se atribuyen todas a metadatos sin clasificarlas.
- Fuera del target Windows x64 MSVC no se aplican los atributos. En WSL,
  `.text`, `.init`, `.fini` y `.plt` mantienen bytes, tamaños y direcciones.
  La comparación de Pi/Jelly da +0,17 % y -0,28 % de reducción de tiempo,
  respectivamente: no se observa una regresión comparable a Windows.
- Fuente base: `b9b64e3` / 1.6.6. La versión permanece 1.6.6 con este arreglo
  local; no se sustituye el ZIP publicado ni se crea una nueva release.

| Binario | SHA256 |
| --- | --- |
| Control Windows 1.6.6 | `afe8bcffafa8adf1456fb1a13eed93e9a5dece8a994523c879dcddd09396c488` |
| Windows corregido | `ee48341de3c6dcdece2081a1ff7e3babfa16c0922ea00301c0f2ac463b7d8e9b` |
| WSL corregido | `7701fb8e41d99c3f0865d3e3a368963ef628b8bf9c896fcaa1fa9913c4114961` |

Los ejecutables corregidos están en `target/release/avl-basic.exe` y
`target/release/avl-basic`. Resultados compactos, hashes, configuración y
ejecuciones individuales: [code-placement-results-2026-09-25.json](code-placement-results-2026-09-25.json).
Los programas generados, binarios de ablación, fuentes congeladas, logs y
salidas completas permanecen en `target/perf-regression-20260925/`.

Para comprobar la estructura y repetir la medición con una salida nueva:

```powershell
python tools/benchmarks/check_hot_sections.py target/release/avl-basic.exe
python tools/benchmarks/representative.py --baseline target/perf-regression-20260925/current-1.6.6.exe --candidate target/release/avl-basic.exe --runs 4 --warmups 1 --workloads pimachin,jelly --output target/new-code-placement-check
```

El segundo comando deja la afinidad en manos del sistema. Para replicar LP2 en
Windows, establece afinidad 0x4 en el proceso padre, heredada por ambos hijos;
la investigación conservó el wrapper `target/perf-data-20260918/compare-lp2.py`.
La comprobación estructural no garantiza rendimiento: repetir las cargas
sobre el ejecutable exacto que se vaya a entregar sigue siendo necesario.
