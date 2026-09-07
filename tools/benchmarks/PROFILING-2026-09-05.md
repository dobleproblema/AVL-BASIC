# Diagnóstico del rendimiento del intérprete Rust — 5 de septiembre de 2026

## Decisión

**Recomiendo cerrar esta ronda y conservar el intérprete actual.** Los perfiles permiten localizar el coste, pero las dos modificaciones pequeñas ensayadas en el evaluador numérico empeoran claramente Windows; la segunda también empeora WSL. No seguiría ajustando la representación del resultado de cada expresión ni abriría inmediatamente otra ronda general para perseguir un porcentaje prefijado.

**No hay fundamento para presentar un 5 % general como muy factible, ni para esperar un 15–20 % mediante retoques similares. Tampoco se ha demostrado que el intérprete haya alcanzado su límite o que esos porcentajes exijan una reescritura extensa.** Esa distinción importa: hemos descartado una familia concreta de cambios, no todas las optimizaciones posibles.

Si se dedica más esfuerzo a rendimiento por una necesidad concreta, **priorizaría reducir reservas y copias temporales en programas de strings**, con un objetivo expresamente textual. En esa carga, solo tres funciones del asignador de Windows concentran el 51,52 % de las muestras; las funciones identificadas de reserva/liberación en WSL suman el 21,45 %. Hay una señal concreta para investigar, aunque esos porcentajes no indican cuánto trabajo puede eliminarse ni garantizan una mejora en los programas numéricos.

| Pregunta práctica | Recomendación tras esta ronda |
|---|---|
| ¿Es inútil seguir porque ya está muy optimizado? | No hemos demostrado un techo. Sí hay evidencia suficiente para dejar de insistir en estos ajustes numéricos y en sus variantes inmediatas. |
| ¿Merece la pena perseguir ahora un 5 % general con cambios pequeños? | No como siguiente inversión por defecto. Dos candidatos dirigidos a una región importante han fallado. Haría falta identificar una operación costosa que se pueda eliminar, no solo otra forma de expresarla en Rust. |
| ¿Conviene una reescritura para ganar un 15–20 %? | No la recomiendo con esta evidencia: su coste no está justificado por un beneficio demostrado. |
| ¿Dónde invertiría si existe una necesidad de rendimiento? | Primero en asignaciones temporales de strings, si el uso real incluye bastante texto. Para una necesidad numérica transversal, solo en un prototipo acotado que reduzca recorridos o despacho por nodo, antes de plantear una sustitución mayor. |

Un prototipo que fusionase operaciones frecuentes o preparase una representación ejecutable de expresiones sería una clase de cambio distinta de las probadas aquí. Afectaría a una parte del evaluador, sin implicar necesariamente reescribir todo el intérprete, pero tendría más riesgo semántico y un resultado todavía incierto. **No se ha diseñado ni medido ese prototipo, y no recomiendo comprometer una reescritura basándose únicamente en su posibilidad.**

## Qué se ha medido

La referencia es Rust 1.5.91, commit `2f7c49d7ddf7f51f8636bcc6b97bcb3e1ffae3aa`. Se han conservado los tamaños de la batería anterior y se han separado los perfiles de CPU de las comparaciones de tiempo. La máquina es un Intel Core i7-13700K, con Windows 11 y Ubuntu 24.04 sobre WSL2; compilador Rust 1.95.0, LLVM 22.1.2. Es una sola máquina, no una muestra de distintas arquitecturas ni Linux nativo.

| Carga | Tamaño fijado | Aspectos principales |
|---|---|---|
| Raytracer | 640 × 480 | Aritmética, arrays, control, funciones matemáticas y rasterización |
| Pi Machin | 10.000 dígitos solicitados | Aritmética por bloques, resto/división, arrays y bucles |
| N-Queens | N=16, 16 búsquedas | Arrays, comparaciones y control de ejecución |
| Jelly | 10.000 puntos, 320 fotogramas | Trigonometría, arrays, expresiones y dibujo |
| Mandelbrot | 640 × 480, máximo 80 iteraciones | Evaluación numérica repetida, condiciones y función BASIC |
| Strings | 256 registros, 1.000 pasadas | Funciones anidadas, búsquedas, fragmentos, normalización, concatenación y conversiones |
| Pi moderno | 10.000 dígitos | CALL, variables locales y arrays |
| Matrices | 150.000 secuencias | Transformaciones e inversión de matrices pequeñas |

Las seis primeras cargas tienen tres perfiles por plataforma: **36 capturas, 46.882 muestras de CPU**. Pi moderno y matrices sirven como regresión adicional en las comparaciones Windows. Ya se conocían de la ronda anterior; no son validación inédita aunque el campo histórico del JSON se llame `validation`.

Los gráficos se ejecutan con `AVL_BASIC_WINDOW=0`: se conserva la rasterización y se verifica el PNG final, pero no se mide presentación, sincronización ni respuesta interactiva de una ventana. Tampoco se cubren todos los usos de strings, archivos o contratos del lenguaje.

## Dónde se consume la CPU

Los siguientes son porcentajes exclusivos agrupados de las tres capturas: la muestra cuenta en la función física que contiene la instrucción interrumpida. **No son porcentajes de ahorro disponible.** El compilador integra algunas funciones dentro de otras; por ello `run_from_inner` incluye evaluación y ejecución de comandos integradas y no puede interpretarse como «despacho puro».

| Carga | Evaluador numérico Windows | Evaluador numérico WSL | `run_from_inner` Windows | `run_from_inner` WSL |
|---|---:|---:|---:|---:|
| Raytracer | 46,86 % | 48,29 % | 35,95 % | 37,47 % |
| Pi Machin | 42,16 % | 35,17 % | 32,33 % | 26,35 % |
| N-Queens | 54,88 % | 54,38 % | 18,71 % | 21,74 % |
| Jelly | 50,53 % | 51,43 % | 12,45 % | 11,99 % |
| Mandelbrot | 56,54 % | 57,74 % | 31,24 % | 31,96 % |

El evaluador es `FastNumberExpr::eval`. Su peso confirma que merece atención para una mejora numérica transversal; **no demuestra que una instrucción o comprobación concreta dentro de él sea un cuello de botella eliminable**. La región contiene trabajo necesario, selección de operaciones, accesos y propagación de errores. Las repeticiones sitúan, por ejemplo, Raytracer WSL entre el 47,61 y el 48,70 % y Mandelbrot entre el 55,46 y el 58,97 %: la concentración no procede de una única captura.

Hay costes específicos que limitan una explicación única para todos los programas:

- **Pi Machin WSL:** la implementación de resto de coma flotante (`fmod`) concentra el 22,37 %. Es matemática solicitada por el programa. Sustituirla exigiría preservar su semántica; el perfil no demuestra que pueda eliminarse.
- **N-Queens WSL:** `get_array_number_cached` concentra el 12,04 %. El acceso ya utiliza una ruta cacheada; no sería correcto suponer que todo ese tiempo se gasta buscando nombres en una tabla hash.
- **Jelly:** seno, coseno y arcotangente suman el 23,91 % en WSL; las funciones trigonométricas identificadas, incluida reducción de argumento, el 21,61 % en Windows. Optimizar solo el control del intérprete no elimina ese cálculo.
- **Strings:** en Windows, `RtlpLowFragHeapAllocFromContext` aporta el 25,23 %, `RtlFreeHeap` el 21,21 % y `RtlpAllocateNTHeapInternal` el 5,08 %. Estas tres funciones ya suman el 51,52 %; existen otras contribuciones del heap. En WSL, las funciones identificadas de reserva/liberación suman el 21,45 %, `memcmp` el 11,95 %, el reconocimiento de identificadores reservados el 8,12 % y el evaluador genérico el 7,54 %. La distribución es distinta de la numérica y depende mucho de la plataforma.

La señal de strings respalda investigar la creación y destrucción de temporales, pero todavía no identifica qué sitios de asignación aportan la mayor parte ni qué proporción es evitable. No se han contado asignaciones ni bytes por llamada en esta ronda. Antes de un parche habría que atribuirlas a las operaciones de texto concretas y conservar el tratamiento de caracteres Unicode.

El arranque y la exportación tampoco explican por sí solos el fracaso anterior. En las referencias de la ronda del 4 de septiembre, la mediana de la fracción externa al marcador BASIC estaba entre el 0,50 y el 2,67 % en Windows y entre el 0,58 y el 3,30 % en WSL para estas seis cargas. Esa diferencia de cronómetros es orientativa: incluye preparación y finalización, no mide PNG de forma aislada y utiliza relojes distintos. No es una bolsa oculta de 15–20 % que se haya confundido con interpretación.

## Dos hipótesis contrastadas y descartadas

Se probaron exactamente dos alternativas en copias aisladas de la referencia. Ambas modifican el transporte del resultado de la evaluación numérica, conservando el error completo y el orden de evaluación.

1. **Error en `Box`.** Mantener `BasicResult<f64>` en la frontera y usar internamente `Result<f64, Box<BasicError>>`. La hipótesis era que una representación de retorno menor redujese el trabajo repetido entre evaluaciones recursivas. La reserva de la caja solo ocurre ante un error: **las ejecuciones correctas de estos benchmarks no reservan cajas de error**, por lo que no se debe explicar su regresión como un coste de `Box::new` durante el cálculo normal.
2. **Estado de error compartido.** Devolver directamente `f64` y transportar un único `Option<BasicError>` por la evaluación de la expresión, comprobándolo tras cada hijo. Evita nuevas reservas y conserva la primera excepción. La hipótesis era eliminar el buffer de retorno del número. Supone más comprobaciones explícitas en el código fuente, con mayor cuidado de mantenimiento que la propagación habitual con `?`.

Se midieron compilaciones release normales, sin perfilador: seis repeticiones medidas por binario/carga en Windows y cuatro en WSL, además de un calentamiento por pareja. El orden base/candidato está equilibrado y el orden de cargas rota. Las cuatro comparaciones reúnen 264 ejecuciones medidas y 50 calentamientos.

La métrica es **reducción del tiempo externo monotónico**, `100 × (1 − mediana_candidato / mediana_base)`. Un porcentaje negativo significa que el candidato tarda más. Los agregados usan la media geométrica de las razones de tiempo con igual peso por carga, sin cambiar pesos para favorecer un resultado.

| Carga | Box Windows | Estado compartido Windows | Box WSL | Estado compartido WSL |
|---|---:|---:|---:|---:|
| Raytracer | −3,50 % | −3,28 % | −0,48 % | −2,47 % |
| Pi Machin | −5,75 % | −8,60 % | No medido | −5,81 % |
| N-Queens | −10,26 % | −11,59 % | +2,61 % | −7,98 % |
| Jelly | −7,23 % | −5,76 % | No medido | −1,69 % |
| Mandelbrot | −12,51 % | −15,70 % | +1,60 % | −6,17 % |
| Strings | −0,27 % | +0,91 % | No medido | −0,21 % |
| Pi moderno | −5,08 % | −4,34 % | No medido | No medido |
| Matrices | −0,48 % | +1,69 % | No medido | No medido |
| **Agregado de lo medido** | **−5,56 % (8 cargas)** | **−5,69 % (8 cargas)** | **+1,25 % (3 cargas)** | **−4,02 % (6 cargas)** |

Las coberturas WSL son distintas: **sus agregados no son comparables directamente entre sí ni con los ocho casos Windows**. La primera prueba WSL fue un contraste limitado; las regresiones Windows ya impedían aceptar el candidato. No se amplió su cobertura para buscar una media más favorable. La segunda falla también en las seis cargas de ajuste WSL.

Las pequeñas variaciones de strings o matrices no justifican conservar cambios que empeoran varios programas numéricos entre un 3 y un 16 % en Windows. Son regresiones repetidas y de magnitud suficiente para rechazar las variantes; no hace falta resolver cada causa de compilación para tomar esa decisión. Las medianas, dispersión y tiempos individuales están en el JSON adjunto. No se presentan intervalos de confianza formales ni se convierte una variación pequeña en una ganancia estable.

## Qué explica el código generado y qué sigue sin explicar

La inspección del ensamblador Linux permite contrastar el mecanismo pretendido. La referencia conserva un símbolo identificable; los rangos de los candidatos, despojados de símbolos, se atribuyen al evaluador por su estructura recursiva y selección de nodos. Esa atribución es una inferencia documentada, no un símbolo recuperado.

| Variante | Tamaño de la función identificada | Reserva local de pila | Retorno numérico observado |
|---|---:|---:|---|
| Referencia | 5.224 bytes | 152 bytes | Buffer de memoria |
| Error en Box | 6.041 bytes | 280 bytes | Sigue utilizando un buffer de memoria |
| Estado compartido | 5.642 bytes | 264 bytes | Registro `xmm0`, con estado de error separado |

**La primera variante no consiguió eliminar el retorno por memoria en esta compilación Linux. La segunda sí lo consiguió, pero no mejoró el tiempo.** Las tres conservan doce sitios de llamada recursiva. Cambiar la forma del retorno no elimina el recorrido del árbol, el despacho por nodo ni las comprobaciones posteriores a cada hijo. Además, la referencia no copia el resultado completo de 40 bytes en cada operación correcta: en esa ruta escribe el discriminante y el número; las copias grandes observadas corresponden a propagación de errores.

Esto corrige la hipótesis inicial: hacer menor el tipo o devolver el número por un registro no asegura reducir el trabajo total generado. Se observan funciones y reservas de pila mayores, pero **no se ha demostrado que el tamaño de código, la caché, la predicción de saltos o la presión de registros sean la causa individual de las regresiones**. Tampoco se extrapola el ABI de Linux a Windows. La evidencia detallada está en [el análisis del protocolo de retorno](RETURN-PROTOCOL-2026-09-05.md).

## Calidad del diagnóstico y límites

**Perfilado.** WSL utiliza `perf` 6.8.12, evento software `cpu-clock:u` a 997 Hz y pilas DWARF de 16.384 bytes. Windows utiliza VSDiagnostics de Visual Studio, muestreo de CPU a 1 ms y extracción ETW/TraceEvent filtrada por ejecutable y PID. Las capturas, compilaciones y comparaciones de rendimiento se realizaron secuencialmente. No se fijó una afinidad reducida a una clase de núcleo; los datos corresponden al planificador de esta máquina híbrida.

| Calidad de las capturas finales | Windows | WSL |
|---|---:|---:|
| Capturas | 18 | 18 |
| Muestras | 23.610 | 23.272 |
| Pérdidas registradas | 0 | 0 |
| Muestras sin pila | 0 | 0 |
| Hojas sin símbolo | 36 (0,152 %) | 0 |

En WSL quedan entradas desconocidas en las raíces o extremos de muchas pilas; no se afirma resolución completa de todos sus marcos. En Windows se resolvieron los símbolos del intérprete y se añadieron PDB públicos de Microsoft con identidad verificada. Los porcentajes publicados son exclusivos de funciones físicas; no se suman porcentajes inclusivos de funciones anidadas. Las duraciones bajo perfilador no se usan para cuantificar aceleraciones.

**Sesgo del binario con símbolos.** Se conservaron las opciones de optimización release (`opt-level=3`, LTO thin, una unidad de generación y panic abort), añadiendo información de depuración y evitando retirar símbolos. Eso cambió el código generado. Un control A/B de cuatro repeticiones en las seis cargas encontró una reducción agregada de −2,35 % en Windows y −1,69 % en WSL; Mandelbrot empeoró un 6,90 y un 6,46 %, respectivamente. Por tanto, los perfiles localizan regiones importantes, pero no son porcentajes exactos del binario distribuible ni una predicción cuantitativa del ahorro. Los candidatos se juzgaron con compilaciones normales. Configuración de perfiles: [documentación de Cargo](https://doc.rust-lang.org/cargo/reference/profiles.html); captura Windows: [documentación de Visual Studio](https://learn.microsoft.com/en-us/visualstudio/profiling/profile-apps-from-command-line?view=visualstudio).

**Cronómetros.** Todos los agregados utilizan `elapsed_s`, obtenido con `time.perf_counter()` alrededor del proceso. El marcador interno BASIC `TIME` usa reloj de sistema y sirve solo de diagnóstico secundario. Se detectaron cuatro casos WSL donde `body_s` supera claramente al tiempo externo: Pi y strings en el control de símbolos, y Pi y Raytracer en la variante de estado compartido. Se conservan los valores originales y se marcan como inválidos para analizar `body_s`; no entran en los agregados publicados. Un salto del reloj es una explicación posible, no comprobada. No se ha modificado `TIME` en esta tarea.

**Equivalencia.** Todas las ejecuciones de candidatos y perfiles conservaron stdout y stderr respecto a su referencia, salvo el marcador explícito de tiempo, y los PNG finales coincidieron. Cada variante pasó dos pruebas focalizadas de unidad en release en Windows: resultados numéricos y signo de cero, errores de índices y funciones, orden de evaluación con efectos de `RND`, propagación del primer error y restauración de bindings con manejo de errores. Es una comprobación dirigida, no una validación exhaustiva del lenguaje. No se ejecutó la suite completa para publicar candidatos que ya habían sido descartados.

**Protección frente al sobreajuste.** Se prepararon dos cargas adicionales antes de evaluar posibles ganancias en ellas: `7^10000` desde `powermul.bas` y edición/búsqueda Unicode con buffers de 32, 128, 512 y 2.048 caracteres. Solo se calibraron con la referencia: 0,7361 s y 0,9313 s en la ejecución final Windows. Sus salidas completas se verificaron con un oráculo Python independiente. Unicode se fijó en 30.000 pasadas tras calibrar su duración; no se ajustó según resultados de candidatos. Ningún candidato superó las pruebas previas, de modo que **no se midió ninguno con estas dos cargas**. Permanecen sin utilizar para seleccionar una variante. Esta ronda permite rechazar cambios, no demostrar la generalización de una mejora.

## Evidencia conservada y reproducción

El [JSON consolidado](profiling-results-2026-09-05.json) conserva los agregados, tiempos individuales, calidad de perfiles, identidades de ejecutables y fuentes, anomalías y referencias a los datos originales. Los candidatos de error en Box y estado de error compartido fueron descartados y sus parches se han retirado; **ninguno se incorporó al intérprete**.

Se conservan los datos originales en `C:/Users/josea/Documents/Rust/AVL-BASIC/target/perf-20260905`. El informe Windows definitivo es `profiles-windows-symbolized/results.json`; `profiles-windows/results.json` es la extracción anterior a completar símbolos del sistema. Las trazas `perf.data` están en el sistema de archivos Linux de WSL, `/home/antonio/.cache/avl-perf-20260905/profiles`, y su resumen en `profiles-wsl.json`. Se utilizó ese directorio porque la escritura de la captura directamente en `/mnt/c` fallaba. No se instalaron paquetes globalmente ni se modificó la configuración de permisos de perfilado.

Durante una extracción de `.text`, `objcopy` reescribió metadatos ELF de la referencia histórica Linux. Se verificó que todas las secciones cargables —código, datos, direcciones y tamaños— coincidían con el original, se conservó el archivo exacto usado en el diagnóstico como `baseline-linux-profiled` y se restauró la referencia histórica byte a byte. `baseline-identity.json`, incluido en el consolidado, documenta ambas identidades. Los hashes originales de las mediciones no se han sustituido retrospectivamente. Las comparaciones de candidatos utilizaron la referencia restaurada.

Desde la raíz del repositorio Rust, para repetir la segunda variante Windows con los binarios conservados y una carpeta nueva:

```powershell
python tools/benchmarks/representative.py --baseline target/perf-20260904/baseline-live.exe --candidate target/perf-20260905/shared-error-windows/release/avl-basic.exe --config target/perf-20260905/frozen-config.json --output target/perf-20260905/recheck-shared-windows --runs 6 --warmups 1
```

Desde WSL, en la misma raíz:

```sh
python3 tools/benchmarks/representative.py --baseline target/perf-20260904/baseline-live-linux --candidate target/perf-20260905/shared-error-wsl/release/avl-basic --config target/perf-20260905/frozen-config.json --output target/perf-20260905/recheck-shared-wsl --group tuning --runs 4 --warmups 1
```

Para nuevas capturas Windows, con el binario con símbolos y su control conservados:

```powershell
python tools/benchmarks/profile_windows.py --exe target/perf-20260905/windows-symbols-build/release/avl-basic.exe --pdb target/perf-20260905/windows-symbols-build/release/avl_basic.pdb --programs target/perf-20260905/programs --oracle-results target/perf-20260905/symbols-control-windows/results.json --output target/perf-20260905/profiles-windows-new --runs 3
```

El runner cierra su propia sesión de captura incluso ante fallos. La extracción posterior con los símbolos de sistema exactos está documentada en `target/perf-20260905/WINDOWS-PROFILING.md`. Para nuevas capturas WSL:

```sh
python3 tools/benchmarks/profile_linux.py --perf target/perf-20260905/perf-tools/extracted/usr/lib/linux-tools-6.8.0-139/perf --executable target/perf-20260905/wsl-symbols-build/release/avl-basic --baseline target/perf-20260904/baseline-live-linux --config target/perf-20260905/frozen-config.json --output /home/antonio/.cache/avl-perf-20260905/profiles-new
```

El código de producción, los ejemplos y los archivos Cargo permanecen iguales a la referencia. Los experimentos se construyeron en directorios aislados para Windows y WSL. No se incorporó una optimización, no se sustituyeron los ejecutables de desarrollo por candidatos y no se publicó una versión. El resultado útil de esta ronda es una decisión de descarte con evidencia, las herramientas para medir y una prioridad concreta si en el futuro interesa acelerar programas de texto.
