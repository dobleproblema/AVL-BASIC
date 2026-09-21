# Evaluación directa de índices simples — 18 de septiembre de 2026

Se incorpora una optimización pequeña, limitada a Windows, en `eval_fast_index`: los índices que son constantes o variables se obtienen directamente, sin entrar recursivamente en todo `FastNumberExpr::eval`. Los índices compuestos mantienen el evaluador existente. Se conservan resolución de variables, cachés, comprobación de fracciones, conversión, límites, errores y orden de evaluación. No se cambia la representación de `Interpreter`, de las matrices ni del árbol de expresiones.

La decisión se apoya en una mejora repetida en Windows y una comprobación independiente con programas reservados. No se extiende a Linux: allí el prototipo portable produjo resultados mixtos, incluido un empeoramiento repetido de Mandelbrot. El `cfg` se resuelve al compilar, sin añadir una decisión durante la ejecución.

## Resultado del ejecutable final de Windows

Referencia: el ejecutable local con los cambios de ejes/consola que existía al comenzar esta fase, SHA256 `781310de9bdb74b828ca7d21b8cf1c968586e9adef767fa7a9350a82582886a5`. Candidato: el ejecutable final de la ruta habitual `target/release/avl-basic.exe`, SHA256 `2dcf9bdd43ac5bd37b7b7338b134a333e78abd65705c201ec55c98f1eba40558`. La versión sigue siendo 1.5.96; los hashes distinguen estos binarios de la publicación anterior.

Cuatro repeticiones medidas y un calentamiento por pareja; orden AB/BA equilibrado, rotación de cargas, mismo núcleo P (LP2, afinidad 0x4) del i7-13700K. Se usa el tiempo externo monotónico de todo el proceso. Los tamaños son los fijados antes de este experimento; no se modificaron para favorecer el candidato. No se ejecutaron compilaciones ni otras mediciones simultáneas.

| Carga | Antes (s) | Después (s) | Reducción del tiempo |
| --- | ---: | ---: | ---: |
| Raytracer | 2.315381 | 2.211449 | +4.49 % |
| Pi 10.000 | 1.864123 | 1.714628 | +8.02 % |
| N-Queens | 1.538659 | 1.325962 | +13.82 % |
| Jelly sin ventana | 1.024367 | 0.963036 | +5.99 % |
| Mandelbrot | 0.658876 | 0.621120 | +5.73 % |
| Texto | 1.376135 | 1.382632 | -0.47 % |
| Pi moderno | 2.143768 | 1.993399 | +7.01 % |
| Matrices | 1.000617 | 0.995608 | +0.50 % |

La media geométrica de las razones de tiempo da una reducción del **5.73 %** en estas ocho cargas; no representa una mejora universal. Las diferencias de texto y matrices son pequeñas: texto pasó de +0,04 % de reducción en el prototipo a -0,47 % en el final; matrices pasó de -0,63 % a +0,50 %. No hay una dirección estable en estas rondas. Sus tiempos individuales y MAD se conservan; no se afirma que cualquier diferencia pequeña sea necesariamente ruido.

Los programas reservados, sin haberlos utilizado para elegir o ajustar el candidato, dan:

| Carga | Antes (s) | Después (s) | Reducción del tiempo |
| --- | ---: | ---: | ---: |
| 7^10000 | 0.890882 | 0.782056 | +12.22 % |
| Unicode | 0.973222 | 0.975274 | -0.21 % |

El oráculo Python independiente comprobó las 20 ejecuciones: entero completo y resultados/checksum Unicode coinciden. Después de esta ronda dejan de ser casos inéditos para futuras optimizaciones.

Jelly con ventana real, 10.000 puntos y 1.600 fotogramas, pasa de **306.0 a 327.7 FPS**. El tiempo externo baja de 5.257927 a 4.908952 s (6.64 %). Los FPS son 1.600 dividido por la mediana del tiempo BASIC; aquí todos los marcadores internos son positivos y menores que el tiempo externo, sin las anomalías detectadas en algunas ejecuciones WSL. No se mezclan estas cifras con la prueba sin ventana.

En una pareja adicional frente al **publicado anterior a la regresión**, Pi queda un 1.03 % más lento y Jelly un 0.08 % más rápido (sin ventana). Se ha recuperado buena parte de la pérdida; no se da por identificado el mecanismo original ni por eliminado cualquier residuo en toda carga o equipo.

## Por qué se limita a Windows

El prototipo sin condición de plataforma se midió también en WSL Ubuntu 24.04, con las ocho cargas y cuatro parejas más calentamiento. La mejora agregada fue 1.67 %, pero Mandelbrot empeoró un 2,26 % por razón de medianas y en los cuatro pares: +2,96 %, +1,54 %, +3,76 % y +2,74 % de tiempo. No se oculta esa regresión detrás del agregado. La afinidad WSL fue la CPU virtual 2; no se presupone que equivalga al mismo núcleo físico de Windows.

La versión final conserva la ruta original fuera de Windows. Sus secciones ELF `.text`, `.rodata`, `.data` y `.eh_frame` coinciden en contenido, tamaño y dirección con el control Linux. El ELF completo no es idéntico: `.data.rel.ro` difiere. Un contraste posterior de Raytracer, Mandelbrot y texto arroja reducciones de -0.14 %, +0.22 % y -0.72 %, respectivamente; desaparece la pérdida repetida de Mandelbrot del prototipo portable. No se presenta esta variante final como una optimización de Linux.

Se detectaron tres tiempos BASIC `body_s` mayores que `elapsed_s` en la batería del prototipo WSL. Se conservan como evidencia pero no se usan para valorar rendimiento; todos los resultados comparativos usan `elapsed_s`. Ninguna de esas anomalías corresponde a Mandelbrot.

## Qué permite concluir

Evitar recorridos redundantes en el evaluador es una vía concreta que merece atención: este cambio pequeño mejora de forma repetida las cargas numéricas de Windows, incluida una que no se utilizó para seleccionarlo. No justifica una reescritura general de las estructuras de datos ni extender ahora la especialización a todos los operandos.

Tampoco permite atribuir toda la ganancia a menos llamadas de índices: Mandelbrot no usa arrays y también cambia su rendimiento cuando cambia el evaluador compilado. No se midieron nuevos contadores PMU para este candidato. La mejora está demostrada para los ejecutables/cargas indicados; la contribución exacta de despacho, generación de código y disposición sigue sin aislarse. La causa física de la regresión original continúa abierta.

## Corrección, artefactos y reproducción

- Suite Rust completa del candidato: 678 pruebas pasadas en Windows y 678 en WSL, cero fallos; 7 y 1 ignoradas, respectivamente. Las integraciones opcionales que requieren configurar el oráculo Python no se activaron en estas suites. Los oráculos independientes de las cargas reservadas sí se ejecutaron expresamente.
- Dos pruebas nuevas de contrato cubren hojas Number/Var, ausencia de variable, cero negativo, fracciones, NaN/infinito, saturación i32, errores de lectura, FN/LOCAL/alias y orden de evaluación con consumo exacto de RND. Ambas se repitieron sobre la condición de plataforma final y pasaron en los dos sistemas.
- Todas las comparaciones conservan stdout/stderr normalizados y PNG final. Esta equivalencia no cubre todos los fotogramas intermedios ni sustituye a las pruebas semánticas.
- Ejecutables locales actualizados: `target/release/avl-basic.exe` y `target/release/avl-basic`. No se ha incrementado la versión, empaquetado, creado un commit ni publicado una release. Los cambios anteriores del usuario se conservan.
- Resultados permanentes, tiempos individuales, hashes y anomalías: [index-leaves-results-2026-09-18.json](index-leaves-results-2026-09-18.json). Fuentes congeladas, binarios, programas, logs y parches: `target/perf-data-20260918`. Esos archivos crudos de target no están versionados.

Antes del candidato se reconstruyó un control sin modificar y se comparó con el ejecutable inicial: Pi varió +1,18 % y Jelly +0,48 % en tiempo. Por ello la selección inicial se hizo contra ese control reconstruido, y la decisión final se volvió a medir contra el ejecutable real inicial desde la ruta de compilación habitual. No se confunden esas parejas ni se mezclan sus medianas.

Para repetir la comparación final de Windows en este equipo, desde la raíz Rust y escogiendo una salida nueva:

```powershell
python target/perf-data-20260918/compare-lp2.py --baseline target/perf-data-20260918/current-before.exe --candidate target/release/avl-basic.exe --runs 4 --warmups 1 --output target/recheck-index-leaves
```

El wrapper fija LP2 y llama al `representative.py` existente. No ejecutar otro benchmark o compilación simultáneamente. Las configuraciones de la batería y de los casos reservados se conservaron sin cambios.
