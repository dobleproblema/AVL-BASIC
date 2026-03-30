# tests/test_basic_program.py

import pytest
import re
import sys
import math
from types import SimpleNamespace
from pathlib import Path

ROOT_DIR = Path(__file__).resolve().parent.parent
if str(ROOT_DIR) not in sys.path:
    sys.path.insert(0, str(ROOT_DIR))

from basic import (
    GraphicsWindow,
    DirtyGrid,
    BasicInterpreter,
    KEYWORD_STYLE,
    VARIABLE_STYLE,
    RESET,
    ReturnMain,
    _key_q,
    _GUI_KEYSYM_TO_CODE,
    _tk_event_to_key_codes,
    big_bitmap_font,
    small_bitmap_font,
    syntax_highlight,
)

DEFAULT_SESSION_NOISE_PATTERNS = [
    r'^AVL BASIC v1\.5$',
    r'^BASIC interpreter written in Python$',
    r'^Copyright 2024-2026 José Antonio Ávila$',
    r'^License: GPLv3 or later \(see COPYING\)$',
    r'^This is free software under GPLv3 or later\. You may redistribute it under its terms\.$',
    r'^This program comes with ABSOLUTELY NO WARRANTY\. See COPYING\.$',
    r'^Secuencias de escape ANSI no soportadas\.$',
    r'^Saliendo del intérprete BASIC\.$',
    r'^Ready$',
]


@pytest.mark.parametrize(
    ("value", "fmt", "expected"),
    [
        (3, "+0###", "+0003"),
        (-3, "+0###", "-0003"),
        (3, "+####", "   +3"),
        (-12, "+0###.###", "-0012.000"),
        (0, "+####", "   +0"),
    ],
)
def test_format_using_force_sign(value, fmt, expected):
    # Asegura que los nuevos formatos con '+' inicial siempre colocan el signo pegado a los dígitos.
    # Se limpia la caché para evitar resultados heredados de ejecuciones previas.
    BasicInterpreter.format_using.cache_clear()
    assert BasicInterpreter.format_using(value, fmt) == expected


@pytest.mark.parametrize(
    ("value", "fmt", "expected"),
    [
        (-0.00000023, "##.###", " 0.000"),
        (-0.00000023, "#.###", "0.000"),
        (-0.00000023, ".###", ".000"),
        (7.243, ".###", "7.243"),
        (-7.2449, ".###", "-7.245"),
    ],
)
def test_format_using_rounding_and_no_integer_slot(value, fmt, expected):
    BasicInterpreter.format_using.cache_clear()
    assert BasicInterpreter.format_using(value, fmt) == expected


@pytest.mark.parametrize(
    ("value", "fmt", "expected"),
    [
        (4567, "#,###,###", "    4,567"),
        (45, "#,###,###", "       45"),
        (1234567.23, "#,###,###.##", "1,234,567.23"),
        (1234567.23, ",#,###,###.##", "1.234.567,23"),
        (45, "#,##", " ,45"),
        (45, "A,###", "A, 45"),
        (12345, "0.00^^^^", "1.23E+04"),
        (12345, ",0.00^^^^", "1,23E+04"),
        (1e308, "0.00^^^^", "1.00E+308"),
        (1e308, "0.00^^^^^", "1.00E+308"),
        (1e-15, "0.###############", "0.000000000000001"),
        (1.234567890123456e-10, "0.####################", "0.00000000012345678901"),
    ],
)
def test_format_using_grouping_locale_exponential_and_precision(value, fmt, expected):
    BasicInterpreter.format_using.cache_clear()
    assert BasicInterpreter.format_using(value, fmt) == expected


@pytest.mark.parametrize("program_code, expected_output", [
# (
# '''10 T=TIME
# 20 EVERY 25,0 GOSUB 50 'Cada medio segundo
# 30 PRINT "Pausa de 10s (pero con interrupciones)..." : PAUSE 1250
# 40 PRINT "Fin de la pausa."; ROUND(TIME-T,1) : END
# 50 PRINT ">> ISR: "; ROUND(TIME-T,1) : RETURN''',

# '''Pausa de 10s (pero con interrupciones)...
# >> ISR:  0.5
# >> ISR:  1
# Fin de la pausa. 1.3'''
# ),
# (
# '''10 AFTER 40,2 GOSUB 100 '0,8 segundos
# 20 t=TIME : PRINT "Esperando..." : FOR i=1 TO 500_000 : z=z+1 : NEXT
# 30 PRINT "REMAIN del 2 (y lo desactiva):"; REMAIN(2) : CANCEL 2
# 40 PRINT "Si lees esto sin 'demasiado tarde', AFTER fue cancelado" : END
# 100 PRINT "Demasiado tarde. ISR AFTER (z=";STR$(ROUND(z,-5));")" : RETURN''',

# '''Esperando...
# Demasiado tarde. ISR AFTER (z=400000)
# REMAIN del 2 (y lo desactiva): 0
# Si lees esto sin 'demasiado tarde', AFTER fue cancelado'''
# ),
# (
# '''20 EVERY 10,1 GOSUB 200 'Cada 0,2 s
# 30 FOR k=1 TO 2
# 40   DI
# 50   FOR i=1 TO 150_000 : w=w+1 : NEXT i  ' Sección crítica: no queremos ISR aquí
# 60   EI
# 70 NEXT k
# 80 PRINT "Hecho, c=";c : END
# 200 c=c+1 : RETURN''',

# '''Hecho, c= 2'''
# ),
# (
# '''10 PRINT "Armando 0 y 3"
# 15 t=TIME
# 20 AFTER 10,0 GOSUB 100     'Tras 0,2 s
# 30 EVERY 25,3 GOSUB 200     'Cada .5 s, mayor prioridad
# 40 IF TIME-t<2 GOTO 40 ELSE END
# 100 PRINT "ISR P0 INICIO" : FOR i=1 TO 450_000 : w=z+1 : NEXT
# 101 DI
# 102 PRINT "ISR P0 FIN" : RETURN
# 200 PRINT ">>> ISR P3 (interrumpe a P0)"; ROUND(TIME-t,1) : RETURN''',

# '''Armando 0 y 3
# ISR P0 INICIO
# >>> ISR P3 (interrumpe a P0) 0.5
# >>> ISR P3 (interrumpe a P0) 1
# ISR P0 FIN
# >>> ISR P3 (interrumpe a P0) 1.5'''  
# ),
# (
# '''5 t=TIME
# 10 EVERY 50,2 GOSUB 200
# 20 x=x+1 : IF x<1000000 GOTO 20 ELSE END
# 200 t2=TIME-t : PRINT ">";ROUND(t2,1) : IF t2>3 THEN PAUSE 500:PRINT REMAIN(2):CANCEL 2:EI
# 205 RETURN''',

# '''> 1
# > 2
# > 3
#  25'''  
# ),
# (
# '''10 DEF FNSOL$=CHR$(INT(RND*26+97))
# 15 s$=FNSOL$
# 20 EVERY 50,1 GOSUB 60 'EVERY 10
# 25 AFTER 100,0 GOSUB 50 'AFTER250
# 30 PRINT "¿'Adivinaré' la letra que yo mismo he escogido en 2 segundos?"
# 35 IF a$<>s$ THEN 35
# 40 PRINT : PRINT "'";UPPER$(a$);"' es correcto. ¡Yo gano!"
# 45 BEEP : END
# 50 PRINT : PRINT "Demasiado tarde. ¡También gano yo! :DDD"
# 55 END
# 60 a$=FNSOL$
# 65 'PRINT UPPER$(a$);" ";
# 70 RETURN''',

# '''¿'Adivinaré' la letra que yo mismo he escogido en 2 segundos?

# Demasiado tarde. ¡También gano yo! :DDD'''    
# ),
(
'''10 PRINT USING "#,###,###";4567
20 PRINT USING ",#,###,###.##";1234567.23
30 PRINT USING "0.00^^^^";12345
40 PRINT USING "0.###############";1E-15
50 PRINT DEC$(1E-15,"0.###############")''',

'''    4,567
1.234.567,23
1.23E+04
0.000000000000001
0.000000000000001'''
),
(
'''1  T=TIME
5  DI
10 EVERY 25,3 GOSUB 200
20 PRINT "Main DI activo..." : PAUSE 1500
30 EI : PRINT "EI hecho" : PAUSE 1000
40 END
200 PRINT ">>> P3 ";ROUND(TIME-T,1) : RETURN''',

'''Main DI activo...
>>> P3  1.5
EI hecho
>>> P3  2
>>> P3  2.5'''   
),
(
'''10 AFTER 20 GOSUB 50
20 x=REMAIN(0)
30 IF l<>x THEN l=x:PRINT l;
40 GOTO 20
50 PRINT : PRINT "Fin del temporizador"
60 END''',

''' 20  19  18  17  16  15  14  13  12  11  10  9  8  7  6  5  4  3  2  1 
Fin del temporizador'''  
),
(
'''10 EVERY 12 GOSUB 90 'Función intrrumpida por una ISR
20 DEF FNTESTER
30 PRINT "->"
40 PAUSE 1000
50 PRINT "<-"
60 FNEND
70 W = FNTESTER
80 END
90 PRINT "------>"
100 RETURN''',

'''->
------>
------>
------>
------>
<-'''
),
(
'''100 DEF FNFACT(N) 'ON ERROR dentro de la función. No se permite, los ON ERROR son globales
110 ON ERROR GOTO 180
120 R=1
130 IF N<=1 THEN R=R/0:GOTO 200
140 FOR I=1 TO N
150 R=R*I
160 NEXT
170 FNFACT=R : GOTO 200
180 PRINT "ERROR";ERR;"EN LA LINEA";ERL
190 RESUME NEXT
200 FNEND
210 PRINT FNFACT(0)
220 END''',

'''Line 110. Instruction not allowed inside a function.'''   
),
(
'''10 A=99 : B=77 'Variables globales con el mismo nombre que argumentos
20 DEF FNINC(A,B)
30 A=A+100 : B=B+100 : FNINC=A+B
40 FNEND
50 PRINT A;B
60 PRINT FNINC(1,2)
70 PRINT A;B''',

''' 99  77
 203
 99  77'''
),
(
'''10 D=10 'Variable global modificada por la función
20 DEF FNG(A)
30 D=D+A : FNG=D
40 FNEND
50 PRINT FNG(5)
60 PRINT D''',

''' 15
 15'''    
),
(
'''10 DEF FNHELLO$(N$) 'Función con un argumento de tipo cadena
20 FNHELLO$="Hola "+N$
30 FNEND
40 PRINT FNHELLO$("Pepe")''',

'''Hola Pepe'''
),
(
'''10 DEF FNHELLO$(N$,P) 'Argumentos de distinto tipo
20 FNHELLO$="Hola "+N$+" "+STRING$(5,STR$(P))
30 FNEND
40 PRINT FNHELLO$("Pepe",3)''',

'''Hola Pepe 33333'''
),
(
'''10 DEF FNN 'Función sin argumentos
20 N=N+1 : FNN=N
30 FNEND
40 N=10
50 PRINT FNN
60 PRINT FNN()''',

''' 11
 12'''
),
(
'''10 DEF FND(A) 'No asignamos nada a FND
20 IF A>0 THEN A=A
30 FNEND
40 PRINT FND(3)''',

''' 0'''
),
(
'''
10 DEF FNR(X)=FNR(X-1)+1 ' Intento de recursividad en función de una línea
20 PRINT FNR(3)''',

'''Line 20. Instruction not allowed inside a function.'''
),
(
'''10 DEF FNFACT(N) 'Recursividad directa
20 IF N<=1 THEN FNFACT=1 : FNEND
30 FNFACT = N*FNFACT(N-1)
40 FNEND
50 PRINT FNFACT(5)''',

'''Line 30. Instruction not allowed inside a function.'''
),
(
'''10 DEF FNA(N) 'Recursividad mutua
20 IF N<=0 THEN FNA=0 : FNEND
30 FNA=FNB(N-1)
40 FNEND
50 DEF FNB(N)
60 IF N<=0 THEN FNB=1 : FNEND
70 FNB=FNA(N-1) 'Cuando FNB vuelve a llamar a FNA con FNA ya en la pila de llamadas
80 FNEND
90 PRINT FNA(2)''',

'''Line 70. Instruction not allowed inside a function.'''
),
(
'''10 DEF FNFACT(N) 'Factorial sin recursividad
20 R=1
30 IF N<=1 THEN FNFACT=1 : FNEND
40 FOR I=1 TO N
50 R=R*I
60 NEXT
70 FNFACT=R
80 FNEND
90 PRINT FNFACT(5)''',

''' 120'''
),
(
'''10 DEF FNFACT(N)
20 R=1
30 IF N<=1 THEN GOSUB 500 : GOTO 70
40 FOR I=1 TO N
50 R=R*I
60 NEXT
70 FNFACT=R
80 FNEND
90 PRINT FNFACT(1)
100 END
500 R=19
510 RETURN''',

''' 19'''
),
(
'''100 ON ERROR GOTO 210
110 DEF FNFACT(N)
120 R=1
130 IF N<=1 THEN R=R/0:GOTO 170
140 FOR I=1 TO N
150 R=R*I
160 NEXT
170 FNFACT=R
180 FNEND
190 PRINT FNFACT(1)
200 END
210 PRINT "ERROR";ERR;"EN LA LÍNEA";ERL
220 RESUME NEXT''',

'''ERROR 6 EN LA LÍNEA 130
 1'''   
),
(
'''100 ON ERROR GOTO 210
110 DEF FNFACT(N)
120 K=0 : R=1
130 IF N<=1 THEN R=R/K:GOTO 170
140 FOR I=1 TO N
150 R=R*I
160 NEXT
170 FNFACT=R
180 FNEND
190 PRINT FNFACT(1)
200 END
210 PRINT "ERROR";ERR;"EN LA LÍNEA";ERL
220 K=1 : R=19 : RESUME''',

'''ERROR 6 EN LA LÍNEA 130
 19'''   
),
(
'''100 ON ERROR GOTO 210
110 DEF FNFACT(N)
120 K=0 : R=1
130 IF N<=1 THEN R=R/K
135 GOTO 170
140 FOR I=1 TO N
150 R=R*I
160 NEXT
170 FNFACT=R
175 END
180 FNEND
190 PRINT FNFACT(1)
195 PRINT "hola"
200 END
210 PRINT "ERROR";ERR;"EN LA LÍNEA";ERL
220 K=1 : R=19 : RESUME 195''',

'''ERROR 6 EN LA LÍNEA 130
hola''' 
),
(
'''TRON
10 DEF FNTEST1(A)
15 PRINT A
20 GOTO 30
30 FNEND
40 DEF FNTEST2(A)
45 PRINT A
50 GOTO 30
60 FNEND
70 X=FNTEST1(5)
80 X=FNTEST2(10)
90 END''',

'''[10][40][70][15] 5
[20][30][80][45] 10
[50]
Line 50. Invalid target line.'''
),
(
'''10 DEF FNY(w)
20 FNY=w^2
30 FNEND
40 DEF FNX(w)
50 FNX=FNY
60 FNX=FNX+1
70 FNEND
80 PRINT FNX(2)''',

'''Line 50. Incorrect number of arguments.'''
),
(
'''10 DEF FNY(w)
20 FNY=w^2
30 FNEND
40 DEF FNX(w)
50 FNX=FNY(w)
60 FNX=FNX+1
70 FNEND
80 PRINT FNX(2)''',

''' 5'''  
),
(
'''10 DEF FNF(N)
20 LET FNF=1
30 FOR K=1 TO N
40 FNF = K*FNF
50 NEXT K
60 FNEND
70 PRINT FNF(5)''',

''' 120''' 
),
(
'''10 DEF FNX(A) 'Intento de GOTO desde fuera al interior del cuerpo de la función
30 FNX=A
40 FNEND
45 GOTO 30
50 PRINT FNX(5)
60 END
100 RETURN''',

'''Line 45. Invalid target line.'''
),
(
'''10 DEF FNX(A) 'Intento de GOTO fuera del cuerpo de la función
20 GOTO 100
30 FNX=A
40 FNEND
50 PRINT FNX(5)
60 END
100 RETURN
''',

'''Line 20. Invalid target line.'''
),
(
'''10 DEF FNSQ(X)=X*X 'Función que usa otra función de una línea
20 DEF FNSUMSQ(A,B)
30 FNSUMSQ=FNSQ(A)+FNSQ(B)
40 FNEND
50 PRINT FNSUMSQ(3,4)''',

''' 25'''
),
(
'''20 DEF FNM(a,b) 'Llamadas anidadas
30 LET FNM=a
40 IF a<=b THEN 60
50 LET FNM=b
60 FNEND
100 c1=9 : c2=4 : c3=7
110 PRINT FNM(FNM(c1,c2),FNM(c2,c3))''',

''' 4'''    
),
(
'''20 DEF FNM(a,b) 'Elementos de array como argumentos
30 LET FNM=a
40 IF a<=b THEN 60
50 LET FNM=b
60 FNEND
100 a(1)=9 : a(2)=4 : a(3)=7
110 PRINT FNM(FNM(a(1),a(2)),FNM(a(2),a(3)))''',

''' 4'''
),
(
'''10 DEF FNX(A) 'GOSUB fuera del cuerpo de la función
20 GOSUB 100
30 FNX=A+Z
40 FNEND
50 PRINT FNX(5)
60 END
100 Z=7
110 RETURN''',

''' 12'''
),
(
'''10 DEF FNF(N)
15 LET FNF=1
20 GOSUB 45
25 FOR K=1 TO N
30 FNF = K*FNF
35 PRINT "hola!"
40 NEXT K : GOTO 50
45 PRINT "sub" : RETURN
50 FNEND
55 PRINT FNF(5)
60 GOSUB 45
65 END''',

'''sub
hola!
hola!
hola!
hola!
hola!
 120
Line 60. Invalid target line.'''
),
(
'''10 DIM A(10) 'Función que modifica un array global
20 A(3)=7
30 DEF FNSET(I,V)
40 A(I)=V : FNSET=A(I)
50 FNEND
60 PRINT FNSET(3,9)
70 PRINT A(3)''',

''' 9
 9'''
),
(
'''10 DEF FNA(X,Y) 'Llamada con número incorrecto de argumentos
20 FNA=X+Y
30 FNEND
40 PRINT FNA(5)''',

'''Line 40. Incorrect number of arguments.'''
),
(
'''10 DEF FNA(X,Y) 'Llamada con número incorrecto de argumentos
20 FNA=X+Y
30 FNEND
40 PRINT FNA''',

'''Line 40. Incorrect number of arguments.'''
),
(
'''10 DEF FNM(S$) 'Llamada con tipo de argumento incorrecto
20 FNM=LEN(S$)
30 FNEND
40 PRINT FNM(3)''',

'''Line 40. Invalid value type.'''
),
('''10 DEF FNR(A)
20 LET FNR=A+1
30 FNEND
40 PRINT FNR(4)''',

''' 5'''
),
('''10 DEF FNX() 'Intento de CHAIN dentro de una función
20 CHAIN "tests/fixtures/CM-TEST1.BAS"
30 FNEND
40 PRINT FNX()''',

'''Line 20. Instruction not allowed inside a function.'''
),
(
'''10 DEF FNX() 'Intento de MERGE dentro de una función
20 MERGE "tests/fixtures/cm-test2.bas"
30 FNEND
40 PRINT FNX()''',

'''Line 20. Instruction not allowed inside a function.'''
),
(
'''10 DEF FNMIFUNC(A,B)
20 IF A<=0 THEN A=0 : GOTO 60
30 WHILE A>0
40 B=B+1 : A=A-1
50 WEND
60 FNMIFUNC=B
70 FNEND
80 PRINT FNMIFUNC(5,5)''',

''' 10'''
),
(
'''5 PRINT FNMIFUNC(5,5) 'Llamada antes de definir la función
10 DEF FNMIFUNC(A,B)
20 IF A<=0 THEN A=0 : GOTO 60
30 WHILE A>0
40 B=B+1 : A=A-1
50 WEND
60 FNMIFUNC=B
70 FNEND''',

'''Line 5. Undefined variable or function.'''
),
(
'''10 DEF FNMIFUNC(A,B) 'Función incluye la definición de otra función
15 DEF FNSQ(X)=X^2
20 IF A<=0 THEN A=0 : GOTO 60
30 WHILE A>FNSQ(1)
40 B=B+1 : A=A-1
50 WEND
60 FNMIFUNC=B
70 FNEND
80 PRINT FNMIFUNC(5,5)
85 PRINT FNSQ(2)''',

'''Line 15. Instruction not allowed inside a function.'''
),
(
'''10 DEF FNMIFUNC(A,B) 'Función redefinida sobrescribe la original
20 IF A<=0 THEN A=0 : GOTO 60
30 WHILE A>FNSQ(1)
40 B=B+1 : A=A-1
50 WEND
60 FNMIFUNC=B
70 FNEND
75 DEF FNMIFUNC(X)=X^2
80 PRINT FNMIFUNC(2)
85 PRINT FNMIFUNC(5,5) 'Intento de llamar a la función redefinida con los argumentos de la original''',

''' 4
Line 85. Incorrect number of arguments.'''
),
(
'''10 FOR A=1 TO 3
15 IF A=1 THEN
20   PRINT "UNO"
25 ELSEIF A=2 THEN
30   PRINT "DOS"
35   IF A*2=4 THEN
40     PRINT "DOBLE"
45   ELSE
50     PRINT "NO"
55   END IF
60 ELSE
65   PRINT "OTRO"
70 END IF
75 NEXT
80 END''',

'''UNO
DOS
DOBLE
OTRO'''
),
('''10 A=1 : B=1
15 IF A=1 THEN
20   IF B=1 THEN
30     PRINT "A AND B"
40     A=2 : GOTO 80
50   ELSE
60     PRINT "INNER ELSE"
70   END IF
80 ELSEIF A>0 THEN
90   PRINT "OUTER ELSEIF"
100 ELSE
110   PRINT "OUTER ELSE"
120 END IF
130 PRINT "DONE"''',

'''A AND B
DONE'''
),
('''10 A=1
20 IF A=1 THEN
30   PRINT "OK"
40 END IF
50 END IF''',

'''OK
Line 50. END IF without matching IF.'''
),
('''10 PRINT "START"
20 IF 1 THEN
30 PRINT "INSIDE"
40 PRINT "AFTER"''',

'''START
INSIDE
AFTER
Line 20. IF without matching END IF.'''
),
(
'''10 FOR x=1 TO 10
20 PRINT x;
30 IF x=5 THEN STOP
40 NEXT x
RUN
CONT''',

''' 1  2  3  4  5 
 6  7  8  9  10 
 1  2  3  4  5 '''   
),
(
'''10 FOR x=1 TO 10
20 PRINT x;
30 IF x=5 THEN END
40 NEXT x
RUN
CONT''',

''' 1  2  3  4  5 
There is no stopped program to continue.
 1  2  3  4  5 '''   
),
(
'''TRON
10 k=10
20 FOR j=1 TO 2
30 l=k+10
40 PRINT j;k;l
50 k=k+10
60 NEXT
70 END''',

'''[10][20][30][40] 1  10  20
[50][60][30][40] 2  20  30
[50][60][70]'''
),
(
'''TRON
110 DEF FNFACT(N)
120 R=1
130 IF N<=1 THEN 170
140 FOR I=1 TO N
150 R=R*I
160 NEXT
170 FNFACT=R
180 FNEND
190 PRINT FNFACT(5)
200 END''',

'''[110][190][120][130][140][150][160][150][160][150][160][150][160][150][160][170][180] 120
[200]'''
),
(
'''30 DIM A$(2), B$(2)
50 A$(1)="ESTO LO IMPRIMIRÁ"
60 A$(2)="EL PROGRAMA ENLAZADO."
70 B$(1)="1" : B$(2)="2"
80 CHAIN "tests/fixtures/CM-TEST6.BAS"
90 PRINT : PRINT B$(1) : PRINT B$(2) : PRINT
100 END''',

'''
ESTO LO IMPRIMIRÁ EL PROGRAMA ENLAZADO.

ESPECIFICAR UN NÚMERO DE LÍNEA DE ENTRADA
EVITA LAS LÍNEAS QUE NO NOS INTERESAN
'''
),
(
'''10 DEF FNSQ(X)=X^2
11 Z=5
15 A$="tests/fixtures/cm-test1.bas"
20 CHAIN MERGE a$, 20
RUN
LIST''',

''' 25
10 DEF FNSQ(X)=X^2
11 Z=5
15 A$="tests/fixtures/cm-test1.bas"
20 DEF FNCU(X)=X^3
30 PRINT FNSQ(Z)
 25'''    
),
(
'''10 DEF FNSQ(X)=X^2
11 Z=5
12 A=6
13 B=7
14 C=8
15 A$="tests/fixtures/cm-test1.bas"
20 CHAIN MERGE a$, 20, DELETE 12-14
RUN
LIST''',

''' 25
10 DEF FNSQ(X)=X^2
11 Z=5
15 A$="tests/fixtures/cm-test1.bas"
20 DEF FNCU(X)=X^3
30 PRINT FNSQ(Z)
 25'''    
),
(
'''10 A=5 : b=10
20 MERGE "tests/fixtures/cm-test2.bas"                                                                          
30 GOSUB 100                                                                                 
40 MERGE "tests/fixtures/cm-test3.bas"                                                                          
50 GOSUB 200                                                                                 
60 END
RUN
LIST''',

''' 5
 10
10 A=5 : b=10
20 MERGE "tests/fixtures/cm-test2.bas"
30 GOSUB 100
40 MERGE "tests/fixtures/cm-test3.bas"
50 GOSUB 200
60 END
100 PRINT A
110 RETURN
200 PRINT b
210 RETURN
 5
 10'''
),
('''10 A=5 : b=10
20 CHAIN "tests/fixtures/cm-test2.bas"
30 GOSUB 100
40 CHAIN "tests/fixtures/cm-test3.bas"
50 GOSUB 200
60 END
RUN
LIST''',

''' 5
Line 110. RETURN without matching GOSUB.
100 PRINT A
110 RETURN
 0
Line 110. RETURN without matching GOSUB.''' 
),
(
'''1 REM Debe combinarse con tests/fixtures/CM-TEST4.BAS, arrancar en 20 y borrar la 18 original
10 DEF FNSQ(X)=X^2
11 REM A$ es para probar la expresión como argumento
12 DATA "a", "b"
13 READ B$
15 CHAIN MERGE A$+"tests/fixtures/CM-TEST4.BAS", 20, DELETE 18-18
18 PRINT "hola"
20 PRINT "ORIG20"      ' Se elimina por DELETE antes de mezclar
25 READ R1$
26 PRINT "R1=";R1$
28 READ R2$
29 PRINT "R2=";R2$
31 READ R1$ : PRINT "R1=";R1$
35 PRINT "END BASE"
99 END
RUN
LIST''',

'''R1=a
R2=b
SUMA= 12
R1=c
END BASE
1 REM tests/fixtures/CM-TEST4.BAS - Define FNCU y una linea de prueba + DATA
10 DEF FNSQ(X)=X^2
11 REM A$ es para probar la expresión como argumento
12 DATA "a", "b"
13 READ B$
15 CHAIN MERGE A$+"tests/fixtures/CM-TEST4.BAS", 20, DELETE 18-18
20 DEF FNCU(X)=X^3
25 READ R1$
26 PRINT "R1=";R1$
28 READ R2$
29 PRINT "R2=";R2$
30 PRINT "SUMA=";FNSQ(2)+FNCU(2)
31 READ R1$ : PRINT "R1=";R1$
35 PRINT "END BASE"
40 DATA "c", "d"
99 END
R1=a
R2=b
SUMA= 12
R1=c
END BASE'''
),
(
'''10 FOR n=4 TO -4 STEP -1
20 PRINT "Con <decimales> =";n;" -> ";
30 PRINT ROUND(1234.5678,n)
40 NEXT
50 PRINT ROUND(2.5)
60 PRINT ROUND(3.5)''',

'''Con <decimales> = 4  ->  1234.5678
Con <decimales> = 3  ->  1234.568
Con <decimales> = 2  ->  1234.57
Con <decimales> = 1  ->  1234.6
Con <decimales> = 0  ->  1235
Con <decimales> =-1  ->  1230
Con <decimales> =-2  ->  1200
Con <decimales> =-3  ->  1000
Con <decimales> =-4  ->  0
 3
 4'''
),
(
'''10 PRINT 123 XOR 23
20 PRINT 45 AND 17
30 PRINT 128 OR 79
40 PRINT 123 AND 23
50 PRINT 23.0 AND 1e-3
60 PRINT 23.8 AND 1e3
70 PRINT NOT(123 AND 23)
75 PRINT NOT(123 AND 23) OR 0
80 PRINT NOT((45 AND 17) AND 23)
90 PRINT NOT(45 AND 17) XOR 23
100 PRINT 45 XOR 123 AND 145
110 PRINT (45 XOR 123) AND 145
120 PRINT 45 OR 123 AND 145
130 PRINT (45 OR 123) AND 145
140 PRINT 45 AND 123 OR 145
150 PRINT (45 AND 123) OR 145''',

''' 108
 1
 207
 19
 0
 8
-20
-20
-2
-23
 60
 16
 61
 17
 185
 185'''    
),
(
'''10 PRINT 7=7
20 PRINT 8=5
30 PRINT 8=5 OR 7=7'
40 PRINT NOT 5=2
50 PRINT NOT (5=2)''',

'''-1
 0
-1
 0
-1'''    
),
(
'''10 PRINT 3.8 AND 7.2
20 PRINT 3.8 OR 7.2
30 PRINT 3.4 XOR 2.6''',

''' 4
 7
 0'''    
),
(
'''10 X=4 : Y=5
20 PRINT X>=4 AND Y=5
25 PRINT NOT (X>=4 AND Y=5) XOR 23
30 X=3 : Y=5
40 PRINT X>=4 AND Y=5
45 PRINT NOT (X>=4 AND Y=5) XOR 23''',

'''-1
 23
 0
-24'''    
),
(
'''10 DATA "": READ A$: PRINT ">"+A$+"<"''',

'''><'''
),
(
'''10 DATA ,
20 READ A$,B$
30 PRINT ">"+A$+"<"
40 PRINT ">"+B$+"<"''',

'''><
><'''
),
(
'''10 DATA
20 READ A$
30 PRINT LEN(A$)''',

''' 0'''
),
(
'''10 DATA &H, &X
20 READ A,B
30 PRINT A;B''',

'''Line 20. Invalid value type.'''
),
(
'''FOR D2=2 TO 6 STEP 2 : FOR D=1 TO 2 : PRINT D2,D : NEXT D : NEXT D2''',

''' 2\t 1
 2\t 2
 4\t 1
 4\t 2
 6\t 1
 6\t 2'''
),
(
'''D2=2 : WHILE D2<=6 : D=1 : WHILE D<=2 : PRINT D2,D : D=D+1 : WEND : D2=D2+2 : WEND''',

''' 2\t 1
 2\t 2
 4\t 1
 4\t 2
 6\t 1
 6\t 2'''
),
(
'''10 x=-2.5
20 y=-2.5
30 PRINT (x^2+3*y^2)*EXP(1-x^2-y^2)
40 PRINT (x*x+3*y*y)*EXP(1-x*x-y*y)
50 DEF FNZ(x,y)=(x*x+3*y*y)*EXP(1-x*x-y*y)
60 DEF FNK(x,y)=(x*x+3*y*y)*EXP(1-x^2-y^2)
70 PRINT FNZ(x,y)
80 PRINT FNK(x,y)''',

''' 0.00025325233996577
 0.00025325233996577
 0.00025325233996577
 0.00025325233996577'''
),
(
'''10 DEF FNW(hola,adios,adios$)=hola+adios+LEN(adios$)
20 DEF FNP(a)=FNW(a,a+5,STR$(a))+5
30 PRINT FNW(1,2,"hola")
40 PRINT FNP(501)''',

''' 7
 1015'''    
),
(
'''10 x=-2.5 : y=-2.5 : a(3)=2 : a(4)=1 : V=2 : H=7 
15 PRINT (x^2+3*y^2),(1-x^2-y^2)
20 PRINT x^2+3*y^2, 1-x^2-y^2
25 PRINT 2^(3^2)
30 PRINT ">";INT(2.3)^3^2;SPC(3);"<"
35 PRINT ">";INT(2.3)^(3^2);SPC(3);"<"
40 PRINT 2^3^2 'En BASIC asocitatividad de izda. a derecha
45 PRINT (2^3)^2
50 PRINT 2^3^2^2
55 PRINT a(3)^3^2
60 PRINT 2^3^a(3)
65 PRINT 2^(3^a(3))
70 PRINT 2^(a(3)+1)^2
75 PRINT ((A(A(A(4)+1))+2)^(a(4)+1)^(2^(a(3)+1)))^a(3)^a(2+(1+1))^(2^(a(3)+1))
80 PRINT ((A(A(A(4)+1))+2)^(a(4)+1)^2^(a(3)+1))^a(3)^a(2+(1+1))^(2^(a(3)+1))
85 A(7)=2 : PRINT 2^(3+3^(2^2)^1)^(A((2^2+2+(1^1))))
90 PRINT 2^2^2^2^2^2^2^2^2
95 PRINT SQR(5*((((1+0.2*(V/661.5)^2)^3.5-1)*(1-6.875E-6*H)^-5.2656+1)^0.286-1))
100 PRINT -        (        2 ^  3 ^   2)''',

''' 25\t-11.5
 25\t-11.5
 512
> 64    <
> 512    <
 64
 64
 4096
 64
 64
 512
 64
 1.1579208923732E+77
 6.2771017353867E+57
 3.7414441915671E+50
 1.1579208923732E+77
 0.0030253262376767
-64'''
),
(
'''10 DEF FNT=PI*2
20 PRINT FNT
30 PRINT FNT(3)''',

''' 6.2831853071796
Line 30. Incorrect number of arguments.'''
),
(
'''10 DEF FNT=PI*2
15 DEF FNLOL=PI/2
20 PRINT FNT
25 PRINT FNLOL-1''',

''' 6.2831853071796
 0.5707963267949'''
),
(
'''10 DIM V(1000)
20 D(1)=2 : D(2)=3 : D(3)=5
30 DEF FNA(I,J,K)=((I-1)*D(2)+(J-1))*D(3)+K
50 FOR I=1 TO D(1)
60 FOR J=1 TO D(2)
70 FOR K=1 TO D(3)
80 LET V(FNA(I,J,K))=I+2*J+K^2
90 PRINT USING " ## ";V(FNA(I,J,K));
100 NEXT K
105 PRINT
110 NEXT J
115 PRINT
120 NEXT I
999 END''',

'''  4   7  12  19  28 
  6   9  14  21  30 
  8  11  16  23  32 

  5   8  13  20  29 
  7  10  15  22  31 
  9  12  17  24  33 
'''
),
(
'''10 PRINT " A", " B", " C", "GCD"
20 READ A,B,C
30 LET X = A
40 LET Y = B
50 GOSUB 200
60 LET X = G
70 LET Y = C
80 GOSUB 200
90 PRINT A,B,C,G
100 GOTO 20
110 DATA 60, 90, 120
120 DATA 38456, 64872, 98765
130 DATA 32, 384, 72
200 LET Q = INT(X/Y)
210 LET R = X - Q*Y
220 IF R = 0 THEN 300
230 LET X = Y
240 LET Y = R
250 GOTO 200
300 LET G = Y
310 RETURN
320 END''',

''' A\t B\t C\tGCD
 60\t 90\t 120\t 30
 38456\t 64872\t 98765\t 1
 32\t 384\t 72\t 8
Line 20. No more DATA to read.'''
),
(
'''10 READ a$
20 WHILE a$<>"*"
30 PRINT a$
40 READ a$
50 WEND
60 DATA los días de la semana son lunes, martes, miércoles, jueves, viernes y sabado
70 DATA "los días de la semana son lunes, martes, miércoles, jueves, viernes y sábado"
80 DATA *''',

'''los días de la semana son lunes
martes
miércoles
jueves
viernes y sabado
los días de la semana son lunes, martes, miércoles, jueves, viernes y sábado'''
),
(
'''10 ON ERROR GOTO 85
15 T=1E+305
20 PRINT T*T
25 PRINT SQR(2)
30 PRINT SQR(-1)
35 PRINT -1^0.5
40 PRINT (-1)^0.5
45 PRINT ABS((-1)^0.5)
50 PRINT ACS(4)
55 PRINT LOG(-2,4)
60 PRINT 0/0
65 GOTO 1000
70 GOTO
75 a=60 : GOTO a
80 END
85 PRINT "Error";ERR;"en línea";ERL
90 RESUME NEXT''',

'''Error 7 en línea 20
 1.4142135623731
Error 26 en línea 30
-1
Error 4 en línea 40
Error 5 en línea 45
Error 26 en línea 50
Error 31 en línea 55
Error 6 en línea 60
Error 12 en línea 65
Error 10 en línea 70
Error 10 en línea 75'''    
),
(
'''10 t=TIME
20 PAUSE 500
30 t2=TIME
40 PRINT USING "#.#";ROUND(t2-t,1)''',

'''0.5'''    
),
(
'''10 A(3,2)=19
15 V(1,1)=2
20 PRINT A(MAX(3,1),V(MIN(3,1),1))''',

''' 19'''  
),
(
'''10 FOR X=1 TO 0                                                                                                                                                                                                                                                                                                                                                                          
20 FOR Y=1 TO 2                                                                                                                                                                                                                                                                                                                                                                          
30 PRINT X,Y                                                                                                                                                                                                                                                                                                                                                                             
40 NEXT                                                                                                                                                                                                                                                                                                                                                                                  
50 NEXT''',

'''''' 
),
(
'''10 FOR X=1 TO 1                                                                                                                                                                                                                                                                                                                                                                          
20 FOR Y=1 TO 2                                                                                                                                                                                                                                                                                                                                                                          
30 PRINT X;Y                                                                                                                                                                                                                                                                                                                                                                             
40 NEXT                                                                                                                                                                                                                                                                                                                                                                                  
50 NEXT''',

''' 1  1
 1  2'''     
),
(
'''10 FOR i=-2 TO 2
20   FOR j=i TO -1
30     PRINT i;j
40   NEXT j
50 NEXT i''',
                                                                                                                                                                                                                                                                                                                                                                                     
'''-2 -2
-2 -1
-1 -1'''   
),
(
'''10 FOR i=-2 TO 2
20   FOR j=i TO 1 STEP -1
30     PRINT i;j
40   NEXT j
50 NEXT i''',
                                                                                                                                                                                                                                                                                                                                                                                    
''' 1  1
 2  2
 2  1'''    
),
(
'''10 FOR i=-2 TO 2
20   FOR j=i TO -1
30     PRINT i;j
40   NEXT j
50   FOR j=i TO 1 STEP -1
60     PRINT i;j
70   NEXT j
80 NEXT i''',
                                                                                                                                                                                                                                                                                                                                                                                      
'''-2 -2
-2 -1
-1 -1
 1  1
 2  2
 2  1'''    
),
(
'''10 FOR X=1 TO 0
20 FOR Y=1 TO 2
30 kkt=7
40 PRINT X,Y
50 NEXT
60 PRINT Y
70 PRINT kkt
80 NEXT''',

''''''   
),
(
'''10 FOR X=1 TO 0
20 kkt=7
30 PRINT X,Y
40 PRINT Y
50 PRINT kkt
60 NEXT
70 PRINT kkt''',

''' 0'''
),
(
'''10 x=15 : WHILE x>10
20 x=0
25 WHILE x<-5
30 x=x+1 : PRINT x
40 WEND
45 PRINT x
50 WEND''',
                                                                                                                                                                                                                                                                                                                         
''' 0'''    
),
(
'''10 WHILE 5>10
20 x=0
25 WHILE x<5
30 x=x+1 : PRINT x
40 WEND
45 PRINT x
50 WEND''',

''''''
),
(
'''10 WHILE 5>10
20 x=0
25 FOR x=1 TO 5
30 x=x+1 : PRINT x
40 NEXT
45 PRINT x
50 WEND
55 PRINT x''',
                                                                                                                                                                                                                                                                                                                         
''' 0'''    
),
(
'''100 DIM a$(10)
110 x=7
120 a$(5)="Amstrad"
125 a$(7)="VXXV"
130 PRINT "->";a$(5)
140 MID$(a$(x-2),4-1,x-5)=MID$(a$(7),2,2)
150 PRINT "<-";a$(5)''',

'''->Amstrad
<-AmXXrad'''
),
(
'''155 a$="Amstrad"
160 MID$(a$,3,2)="achine"
170 PRINT a$
180 a$="Amstrad"
190 MID$(a$,5)="achine"
200 PRINT a$
205 a$="Amstrad"
210 MID$(a$,3,0)="achine"
215 PRINT a$''',

'''Amacrad
Amstach
Amstrad'''
),
(
'''220 a$="Amstrad"
230 MID$(a$,0,2)="achine"
240 PRINT a$''',

'''Line 230. Out of range.'''
),
(
'''10 a$="Amstrad"                                                                              
20 IF MID$(a$,4,2)="tr" THEN MID$(a$,3,2)=MID$(a$,1,2)                                       
30 PRINT a$''',                                                                                  
                                                                                          
'''AmAmrad'''  
),
(
'''10 x=1 : y=7
20 DIM a$(10)
30 a$(1)="hola" : a$(7)="adios"
40 SWAP a$(x),a$(y)
50 PRINT a$(1)
60 PRINT a$(7)''',
                                                                                          
'''adios
hola'''    
),
(
'''10 X=Y=5
20 PRINT X;Y    
30 IF X==4 AND Y=5 THEN PRINT "¡Cierto!"''',

''' 5  5
Line 30. Syntax error.'''
),
(
'''10 X=Y=5
20 PRINT X;Y
30 IF X=>4 AND Y=5 THEN PRINT "¡Cierto!"''',

''' 5  5
Line 30. Syntax error.'''    
),
(
'''10 X=Y=5
20 PRINT X;Y
30 IF X>=4 AND Y=5 THEN PRINT "¡Cierto!"''',

''' 5  5
¡Cierto!'''   
),
(
'''10 X=5
20 PRINT X
30 CLEAR
40 PRINT X''',
''' 5
 0'''
),
(
'''10 ON ERROR GOTO 100 : REM Error personalizado
20 A$ ="abc" : REM ¡No es una letra!
30 IF LEN(A$)<>1 THEN ERROR 75
40 PRINT "¡Ahora sí! :)"
50 STOP : REM Salimos del programa
100 PRINT "Error"; ERR; "en línea"; ERL
110 IF ERR=75 THEN 120 ELSE 130
120 PRINT "¡Te he dicho UNA letra!"
125 A$="a" : REM A ver ahora...
130 RESUME 30''',

'''Error 75 en línea 30
¡Te he dicho UNA letra!
¡Ahora sí! :)'''    
),
(
'''10 ON ERROR GOTO 100 : REM Error personalizado
20 A$ ="abc" : REM ¡No es una letra!
30 IF LEN(A$)<>1 THEN ERROR 75
40 PRINT "¡Ahora sí! :)"
100 PRINT "Error"; ERR; "en línea"; ERL
110 IF ERR=75 THEN 120 ELSE 130
120 PRINT "¡Te he dicho UNA letra!"
125 A$="a" : REM A ver ahora...
130 RESUME 30''',

'''Error 75 en línea 30
¡Te he dicho UNA letra!
¡Ahora sí! :)
Error 0 en línea 0
Error in error handler: RESUME without ERROR.'''
),
(
'''10 ON ERROR GOTO 100
20 Y=3/0
30 PRINT "adios" : STOP
100 PRINT ERR,ERL
110 CLEAR
120 PRINT ERR,ERL
130 RESUME NEXT''',

''' 6\t 20
 6\t 20
adios'''
),
(
'''10 ON ERROR GOTO 100
20 Y=3/0
30 PRINT "adios" : STOP
100 PRINT ERR,ERL
110 CLEAR
120 PRINT ERR,ERL
125 ERROR 6
130 RESUME NEXT''',
                                                                                                                       
''' 6\t 20
 6\t 20
Error in error handler: Division by zero.'''
),
('''100 J=-2
110 FOR J=9 TO J STEP J
115 PRINT J
120 NEXT J''',

''' 9
 7
 5
 3
 1
-1'''
),
(
'''10 DATA , , ,
20 FOR X=1 TO 5
30 READ A$
40 PRINT ">"+A$+"<"
50 NEXT''',

'''><
><
><
><
Line 30. No more DATA to read.'''
),
(
'''10 DATA ,A ,  
20 FOR X=1 TO 4
30 READ A$
40 PRINT ">"+A$+"<"
50 NEXT''',

'''><
>A<
><
Line 30. No more DATA to read.'''
),
(
'''10 PRINT 5 : DATA hola, adios : REM nada
20 READ a$ : PRINT a$
30 READ a$ : PRINT a$''',

''' 5
hola
adios'''
),
(
'''10 PRINT "A"+CHR$(10)+"B"''',

'''A
B'''
),
(
'''10 DIM A(5),B(5),S$(5)
20 FOR I=1 TO 5
30 READ A(I),B(I),S$(I)
40 NEXT
50 DATA 1,7,Alfredo,3,9,Juan,2,2,Enrique,4,6,Pedro,9,1,Manuel
60 FOR I=1 TO 5
70 PRINT S$(I),":";A(I)*B(I)
80 NEXT''',

'''Alfredo\t: 7
Juan\t: 27
Enrique\t: 4
Pedro\t: 24
Manuel\t: 9'''
),
(
'''10 DIM A(5),B(5),S$(5)
20 FOR I=1 TO 5
30 READ A(I),B(I)
40 NEXT
50 FOR I=1 TO 5
60 READ S$(I)
70 NEXT
80 DATA 1,7,3,9,2,2,4,6,9,1
90 DATA Alfredo,Juan,Enrique,Pedro,Manuel
100 FOR I=1 TO 5
110 PRINT S$(I),":";A(I)*B(I)
120 NEXT''',

'''Alfredo\t: 7
Juan\t: 27
Enrique\t: 4
Pedro\t: 24
Manuel\t: 9'''
),
(
'''10 DIM A(10), A$(10)
20 A(7)=B=C=9
30 A=A+14
40 PRINT A,A(7),B,C''',

''' 14\t 9\t 9\t 9'''
),
(
'''10 DIM A(10), A$(10)
20 A=A(7)=B=C=9
30 A=A+14
40 PRINT A,A(7),B,C
50 B=C=D=5 AND C=9
60 PRINT B,C,D
70 D=8=5 AND 7=7
90 PRINT D
100 X=Y=Z=5
110 PRINT X;Y;Z
120 X=Y=(Z=5)
130 PRINT X;Y;Z''',

''' 23\t 9\t 9\t 9
 5\t 5\t 5
 0
 5  5  5
-1 -1  5'''
),
(
'''10 DIM AB(10)
20 AA=AB(7)=BB1=CC0=9
30 AA=AA+14
40 PRINT AA,AB(7),BB1,CC0
50 BB1=CC0=DD=5 AND CC0=9
60 PRINT BB1,CC0,DD
70 DD=8=5 AND 7=7
90 PRINT DD
100 XC1=YC1=ZC1=5
110 PRINT XC1;YC1;ZC1
120 XC1=YC1=(ZC1=5)
130 PRINT XC1;YC1;ZC1''',

''' 23\t 9\t 9\t 9
 5\t 5\t 5
 0
 5  5  5
-1 -1  5'''
),
(
'''10 DIM A(10)
20 A=9
30 A=A+14
40 PRINT A
70 D=A((A-20)*2)=8=5 OR 7=7
90 PRINT D, A(6)''',

''' 23
-1\t-1'''   
),
(
'''10 DIM A1$(10)
20 A1$=A1$(5)="8=6"
30 PRINT A1$, A1$(5), A1$(6)
40 DIM A1(10)
50 A1=A1(5)="8=6"
60 PRINT A1, A1(5), A1(6)''',

'''8=6\t8=6\t
Line 50. Invalid value type.'''
),
(
'''10 DIM A1$(10)
20 A1$=A1$(5)="8=6"
30 PRINT A1$, A1$(5), A1$(6)
40 DIM A1(10)
50 A1=A1(5)="8=6"
60 PRINT A1, A1(5), A1(6)''',

'''8=6\t8=6\t
Line 50. Invalid value type.'''
),
(
'''10 A=5=5 OR W=9
20 PRINT A
30 A=5=5 AND W=0
40 PRINT A''',

'''-1
-1'''    
),
(
'''10 X=5 : DATA "aaa", bbb, 12+7 : DATA +9.8, 12a, 12b, -7.23e12
15 READ A$,B$,C$,D
20 PRINT X,A$,B$,C$,D''',

''' 5\taaa\tbbb\t12+7\t 9.8'''
),
(
'''10 FOR X=1 TO 5
20 W=X^2 : DATA "aaa", bbb, 12+7 : DATA +9.8, 12a, 12b, 7e3, &HFF, "&HFF"
30 NEXT
40 READ A$,B$,C$,D,D$,E$,E
50 PRINT X;W;A$;B$;C$;D;D$;E$;E
60 READ D : PRINT D
70 READ D : PRINT D''',

''' 6  25 aaabbb12+7 9.8 12a12b 7000
 255
Line 70. Invalid value type.'''
),
(
'''10 A=1
20 PRINT A
30 A$="hello"
40 PRINT A$
50 A=0.0002
60 PRINT A
70 A=2.E-6
80 PRINT A
90 A=.2E-6
100 PRINT A''',

''' 1
hello
 0.0002
 2E-06
 2E-07'''
),
(
'''10 REM Prueba trigonométrica                                                                
20 ON ERROR GOTO 220                                                                        
30 RAD                                                                                      
40 DATA 720, 360, 180, 90, 0, -90                                                           
50 FOR T=1 TO 2                                                                             
60 RESTORE                                                                                  
70 PRINT                                                                                    
80 IF T=1 THEN RAD:PRINT "RADIANES" ELSE DEG:PRINT "GRADOS"                                 
90 FOR A=1 TO 6                                                                             
100 PRINT "******************************"                                                  
110 READ X                                                                                  
120 PRINT "SIN";X; : PRINT SIN(X)                                                           
130 PRINT "COS";X; : PRINT COS(X)                                                           
140 PRINT "TAN";X; : PRINT TAN(X)                                                           
150 PRINT "ASN";X; : PRINT ASN(X)                                                           
160 PRINT "ACS";X; : PRINT ACS(X)                                                           
170 PRINT "ATN";X; : PRINT ATN(X)                                                           
180 PRINT "COT";X; : PRINT COT(X)                                                           
190 NEXT A                                                                                  
200 NEXT T                                                                                  
210 END                                                                                     
220 REM Manejo de errores                                                                   
230 PRINT "ERROR"                                                                           
240 RESUME NEXT''',

'''
RADIANES
******************************
SIN 720 -0.544071696438
COS 720 -0.83903872922237
TAN 720  0.64844646318323
ASN 720 ERROR
ACS 720 ERROR
ATN 720  1.5694074387991
COT 720  1.5421473579962
******************************
SIN 360  0.95891572341431
COS 360 -0.28369109148653
TAN 360 -3.380140413961
ASN 360 ERROR
ACS 360 ERROR
ATN 360  1.5680185561616
COT 360 -0.29584569796855
******************************
SIN 180 -0.80115263573383
COS 180 -0.59846006905786
TAN 180  1.3386902103512
ASN 180 ERROR
ACS 180 ERROR
ATN 180  1.5652408283942
COT 180  0.74699881441404
******************************
SIN 90  0.89399666360056
COS 90 -0.44807361612917
TAN 90 -1.9952004122082
ASN 90 ERROR
ACS 90 ERROR
ATN 90  1.5596856728973
COT 90 -0.50120278338015
******************************
SIN 0  0
COS 0  1
TAN 0  0
ASN 0  0
ACS 0  1.5707963267949
ATN 0  0
COT 0 ERROR
******************************
SIN-90 -0.89399666360056
COS-90 -0.44807361612917
TAN-90  1.9952004122082
ASN-90 ERROR
ACS-90 ERROR
ATN-90 -1.5596856728973
COT-90  0.50120278338015

GRADOS
******************************
SIN 720 -4.8985871965894E-16
COS 720  1
TAN 720 -4.8985871965894E-16
ASN 720 ERROR
ACS 720 ERROR
ATN 720  89.920422579623
COT 720 -2.0414049191494E+15
******************************
SIN 360 -2.4492935982947E-16
COS 360  1
TAN 360 -2.4492935982947E-16
ASN 360 ERROR
ACS 360 ERROR
ATN 360  89.840845466255
COT 360 -4.0828098382988E+15
******************************
SIN 180  1.2246467991474E-16
COS 180 -1
TAN 180 -1.2246467991474E-16
ASN 180 ERROR
ACS 180 ERROR
ATN 180  89.681693388549
COT 180 -8.1656196765977E+15
******************************
SIN 90  1
COS 90  6.1232339957368E-17
TAN 90  1.6331239353195E+16
ASN 90 ERROR
ACS 90 ERROR
ATN 90  89.363406424037
COT 90  6.1232339957368E-17
******************************
SIN 0  0
COS 0  1
TAN 0  0
ASN 0  0
ACS 0  90
ATN 0  0
COT 0 ERROR
******************************
SIN-90 -1
COS-90  6.1232339957368E-17
TAN-90 -1.6331239353195E+16
ASN-90 ERROR
ACS-90 ERROR
ATN-90 -89.363406424037
COT-90 -6.1232339957368E-17'''
),
(
'''100 PRINT RGB(1,2,3),,HEX$(RGB(1,2,3))
110 PRINT RGB("red"),HEX$(RGB("red"))
120 PRINT RGB(&Hff,&H00,&Hff),HEX$(RGB(&Hff,&H00,&Hff))
130 PRINT RGB(&Hff00ff),HEX$(RGB(&Hff00ff))
140 PRINT RGB(1232343),HEX$(RGB(1232343))
150 PRINT RGB(32)
160 PRINT RGB(31)
161 PRINT RGB("4,0,251")
162 PRINT HEX$(RGB("4,0,251"))
163 PRINT HEX$(RGB("4,0,251"),6)
170 PRINT "***********************"
180 PRINT RGB$(1,2,3)
190 PRINT RGB$("red")
200 PRINT RGB$(&Hff,&H00,&Hff)
210 PRINT RGB$(&Hff00ff)
220 PRINT RGB$(1232343),HEX$(RGB(RGB$(1232343)))
230 PRINT RGB$(32)
240 PRINT RGB$(31)''',

''' 66051\t\t10203
 16711680\tFF0000
 16711935\tFF00FF
 16711935\tFF00FF
 1232343\t12CDD7
 32
 16777200
 262395
400FB
0400FB
***********************
1,2,3
255,0,0
255,0,255
255,0,255
18,205,215\t12CDD7
0,0,32
0,0,31'''  
),
(
'''10 PRINT 1, VAL("&hff")
20 PRINT 2, VAL("&hffxx")
30 PRINT 3, VAL("&x11111111")
40 PRINT 4, VAL("&x1111era")
50 PRINT 5, VAL("-.x")
60 PRINT 6, VAL("3.2x5")
70 PRINT 7, VAL("3.2e17")
80 PRINT 8, VAL("3.2e6")
90 PRINT 9, VAL("3.2e6xx")
100 PRINT 10, VAL("--17")
110 PRINT 11, DEC$(-17, "0####")
120 PRINT 12, DEC$(-17, "0####%")
130 PRINT 13, BIN$(16,8)
140 PRINT 14, BIN$(16,2)
150 PRINT 15, BIN$(-16)
160 PRINT 16, HEX$(16,8)
170 PRINT 17, HEX$(255,1)
180 PRINT 18, HEX$(-255)
190 PRINT 19, HEX$(255)
200 PRINT 20, &HFFAB + &H11 + &X1100
210 PRINT 21, &HFF + &X11111111
220 Y = &HFF + &HFF
230 PRINT 22, HEX$(Y), Y, &X11111111 + &X11111111
240 PRINT 23, A$ + "&H1"''',

''' 1\t 255
 2\t 255
 3\t 255
 4\t 15
 5\t 0
 6\t 3.2
 7\t 3.2E+17
 8\t 3200000
 9\t 3200000
 10\t 0
 11\t-0017
 12\t-0017%
 13\t00010000
 14\t00
 15\t11110000
 16\t00000010
 17\tF
 18\tFF01
 19\tFF
 20\t 65480
 21\t 510
 22\t1FE\t 510\t 510
 23\t&H1'''
),
(
'''10 DATA &HFF, mm&hffxx, &X1101, 3e6, &hff+2, "&hff"
20 READ A,B$,C,D,E$
30 PRINT A, HEX$(A)
40 READ F$
45 PRINT F$, VAL(F$)''',

''' 255\tFF
&hff\t 255'''
),
(
'''10 DATA 12, "12"
20 READ A, A$
30 PRINT A, A$''',

''' 12\t12'''
),
(
'''10 DIM A(1)
20 A(0)=10
30 A(1)=11
40 A=12
50 PRINT A(0)
60 PRINT A(1)
70 PRINT A''',

''' 10
 11
 12'''
),
(
'''10 X=1
20 IF X=1 GOTO 40 ELSE X=7:GOTO 50
30 STOP
40 X=2:PRINT"40":GOTO 20
50 PRINT X,"50"''',

'''40
 7\t50'''
),
(
'''10 FOR I=0 TO 10
20 PRINT I
30 IF I=5 THEN 50
40 NEXT I
50 FOR I=0 TO 0
60 PRINT I
70 NEXT I
80 FOR I=1 TO 0 STEP -1
90 PRINT I
100 NEXT I
110 FOR I=1 TO 0
120 PRINT I
130 NEXT I''',

''' 0
 1
 2
 3
 4
 5
 0
 1
 0'''
),
(
'''10 GOSUB 100
20 GOSUB 100
30 END
100 GOSUB 200
110 GOSUB 200
120 RETURN
200 PRINT "hello, world":RETURN''',

'''hello, world
hello, world
hello, world
hello, world'''
),
(
'''10 IF 0=0 THEN GOSUB 100:GOSUB 200
20 PRINT "20"
30 END
100 PRINT "100"
110 GOSUB 200 : RETURN
200 PRINT "200"
210 GOSUB 300 : RETURN
300 PRINT "300" : RETURN''',

'''100
200
300
200
300
20'''   
),
(
'''10 IF 0=0 THEN GOSUB 100:GOSUB 200:PRINT "20"
30 END
100 PRINT "100" : GOSUB 200 : GOSUB 300 : RETURN : RETURN
200 PRINT "200" : GOSUB 300 : RETURN
300 PRINT "300" : RETURN''',

'''100
200
300
300
200
300
20'''   
),
(
'''10 IF 0=1 THEN PRINT "20" ELSE GOSUB 100:GOSUB 200:PRINT "25"
30 END
50 PRINT "50" : RETURN
100 PRINT "100" : GOSUB 200 : GOSUB 300 : RETURN : RETURN
200 PRINT "200" : GOSUB 300 : RETURN
300 GOSUB 50 : RETURN''',
'''100
200
50
50
200
50
25'''
),
(
'''10 DATA "a",b
20 DATA "c","d"
40 READ J$
50 PRINT "j=";J$
60 RESTORE 20
70 FOR I=1 TO 3
80 READ J$,K$
90 PRINT "j=";J$;" k=";K$
100 NEXT I''',

'''j=a
j=c k=d
Line 80. No more DATA to read.'''
),
(
'''10 ON ERROR GOTO 40
20 ERROR 75:PRINT"hola"
30 STOP
40 PRINT"aquí"
50 RESUME NEXT''',

'''aquí
hola'''
),
(
'''10 ON ERROR GOTO 40
20 ERROR 75
25 PRINT"hola"
30 STOP
40 PRINT"aquí"
50 RESUME NEXT''',

'''aquí
hola'''
),
(
'''10 DATA "a",b
20 DATA "c","d"
30 ON ERROR GOTO 200
40 READ J$
50 PRINT "j=";J$
60 RESTORE 20
70 FOR I=1 TO 3
80 READ J$,K$
90 PRINT "j=";J$;" k=";K$
100 NEXT I
200 PRINT "LO CONSEGUIMOS"''',

'''j=a
j=c k=d
LO CONSEGUIMOS'''
),
(
'''10 DIM a(5),b(5),s$(5)
15 RESTORE 80
20 FOR i=1 TO 4
30 READ a(i),b(i)
40 NEXT
45 RESTORE 90
50 FOR i=1 TO 5
60 READ s$(i)
70 NEXT
80 DATA 1, 7, 3, 9, 2, 2, 4, 6, 9, 1
90 DATA Alfredo, Juan, Enrique, Pedro, Manuel
100 FOR i=1 TO 5
110 PRINT s$(i),":";a(i)*b(i)
120 NEXT''',

'''Alfredo\t: 7
Juan\t: 27
Enrique\t: 4
Pedro\t: 24
Manuel\t: 0'''  
),
(
'''5 ON ERROR GOTO 50
10 X=3/0
20 PRINT X/0
25 STOP
50 PRINT "Funciona!"
60 X=5
70 GOTO 20''',

'''Funciona!
Error in error handler: Division by zero.'''
),
(
'''5 ON ERROR GOTO 50
10 X=3/0
20 PRINT X/2
25 STOP
50 PRINT "Funciona!"
60 X=5
70 RESUME 20''',

'''Funciona!
 2.5'''
),
(
'''5 ON ERROR GOTO 50
10 X=3/0
20 PRINT X/2
25 STOP
50 PRINT "Funciona!"
60 X=5
65 PRINT ERL,ERR
70 RESUME 20''',

'''Funciona!
 10\t 6
 2.5'''
),
(
'''10 X=0
20 PRINT X: DATA 5,6,7,9
30 FOR X=1 TO 4
40 READ X:PRINT X
50 NEXT X
60 END''',

''' 0
 5'''
),
(
'''10 X=0
20 PRINT X: DATA 5,6,7,9
30 FOR X=1 TO 4
40 READ Y:PRINT Y
50 NEXT X
60 END''',

''' 0
 5
 6
 7
 9'''
),
(
'''10 X=0
20 PRINT X:DATA 5,6:DATA 7,9
30 FOR X=1 TO 4
40 READ Y:PRINT Y
50 NEXT X
60 END''',

''' 0
 5
 6
 7
 9'''
),
(
'''20 ON ERROR GOTO 70
40 ERROR 200
50 PRINT "hola"
60 GOTO 30
70 x=x+1 : IF x=5 THEN 80
75 IF (ERR=200) AND (ERL=40) THEN PRINT "adios":RESUME
80 ON ERROR GOTO 0
85 PRINT "salir"
90 END''',

'''adios
adios
adios
adios
salir'''
),
(
'''20 ON ERROR GOTO 70
40 ERROR 200
50 PRINT "hola"
60 GOTO 30
70 x=x+1 : IF x=5 THEN 80
75 IF (ERR=200) AND (ERL=40) THEN PRINT "adios":RESUME 0
80 ON ERROR GOTO 0
85 PRINT "salir"
90 END''',

'''adios
adios
adios
adios
salir'''
),
(
'''20 ON ERROR GOTO 70
40 ERROR 200 : PRINT "hola"
60 GOTO 30
70 IF (ERR=200) AND (ERL=40) THEN PRINT "adios":RESUME NEXT
80 ON ERROR GOTO 0
85 PRINT "salir"
90 END''',

'''adios
hola
salir'''
),
('''20 ON ERROR GOTO 70
40 ERROR 200
50 PRINT "hola"
60 GOTO 30
70 IF (ERR=200) AND (ERL=40) THEN PRINT "adios":RESUME 50
80 ON ERROR GOTO 0
85 PRINT "salir"
90 END''',

'''adios
hola
salir'''
),
('''10 ON ERROR GOTO 100
20 FOR x=1 TO 10
30 PRINT x
35 IF x=5 THEN PRINT 5/0
40 NEXT
50 END
100 x=7
105 PRINT "Error:";ERR;"en línea";ERL
110 RESUME NEXT''',

''' 1
 2
 3
 4
 5
Error: 6 en línea 35
 8
 9
 10'''
),
( 
'''10 ON ERROR GOTO 60
20 X=0 : WHILE X<10 : Y=3000/X : PRINT Y
30 X=X+1 : WEND : PRINT "adios"
50 STOP
60 Y=7777 : PRINT "hola" : RESUME   NEXT''',

'''hola
 7777
 3000
 1500
 1000
 750
 600
 500
 428.57142857143
 375
 333.33333333333
adios'''
),
(
'''10 ON ERROR GOTO 60
20 X=0 : WHILE X<10 : Y=3000/X : PRINT Y
30 X=X+1 : WEND : PRINT "adios"
60 Y=7777 : PRINT "hola" : RESUME   NEXT''',

'''hola
 7777
 3000
 1500
 1000
 750
 600
 500
 428.57142857143
 375
 333.33333333333
adios
hola
Error in error handler: RESUME without ERROR.'''
),
(
'''100 ON ERROR GOTO 125 : REM Error personalizado
105 A$ = "aaa"
106 ON ERROR GOTO 0
110 IF LEN(A$)<>1 THEN ERROR 75
115 PRINT ":)"
120 STOP
125 PRINT ERL, ERR
130 IF ERR=75 THEN 135 ELSE 140
135 PRINT "¡Te he dicho UNA letra!"
140 RESUME 105''',

'''Line 110. Error 75'''
),
(
'''10 F=0:TROFF:PRINT:PRINT "TROFF"
20 FOR N=1 TO 8
30 PRINT "Programa funcionando":NEXT
40 IF F=1 THEN END
50 TRON:PRINT:PRINT "TRON"
60 F=1:GOTO 20''',

'''
TROFF
Programa funcionando
Programa funcionando
Programa funcionando
Programa funcionando
Programa funcionando
Programa funcionando
Programa funcionando
Programa funcionando

TRON
[60][20][30]Programa funcionando
[30]Programa funcionando
[30]Programa funcionando
[30]Programa funcionando
[30]Programa funcionando
[30]Programa funcionando
[30]Programa funcionando
[30]Programa funcionando
[40]'''
),
(
'''10 PRINT USING "0##.##";-3.2
20 PRINT USING "0##.##*";3.2
30 PRINT USING "0*##.##";3.2
40 PRINT USING "0##.##-";3.2
50 PRINT USING "*###.##";3.2
60 PRINT USING "##.##";-345.2
70 PRINT USING "*##.##";-345.2
80 PRINT USING "0######.##";-3.2
90 PRINT USING "**###.##";3.2
100 PRINT USING "**0##.##";3.2
110 PRINT USING "*0##.##"; 3.
120 PRINT USING "0##.##"; -3.2
130 PRINT USING "*0##.##"; -3.2
140 PRINT USING "0*##.##"; -3.2
150 PRINT USING "$0##.##"; -3.2
160 PRINT USING "##:##";1234
170 PRINT USING "0#:##";234
180 PRINT USING "#:##";234
190 PRINT USING "#:##";1234
200 PRINT USING "#-#";123
210 PRINT USING "A##B";45
220 PRINT USING "A##B";1234
230 PRINT USING "##-##";5678
240 PRINT USING "**##.##";-12345.67''',

'''-03.20
003.20*
0*03.20
003.20-
*  3.20
-345.20
*-345.20
-000003.20
**  3.20
**003.20
*003.00
-03.20
*-03.20
-*03.20
$-03.20
12:34
02:34
2:34
12:34
12-3
A45B
A1234B
56-78
**-12345.67'''
),
(
'''10 REM Formateo con coma, punto y coma y USING
20 CLEAR
30 DIM A(10),B(10)
40 A1=0 : B1=0
50 A$="############.##" : B$="######.##"
60 DIM A(10),B(10)
70 A1=0 : B1=0
80 FOR I=1 TO 4
90 READ A(I),B(I)
100 PRINT USING A$;A(I),,B(I)
110 A1=A1+A(I) : B1=B1+B(I)
120 NEXT I
130 PRINT STRING$(16,"-"),STRING$(16,"-")
140 PRINT USING B$;"TOTAL=";A1,,"TOTAL=";B1
150 DATA 5.8052, 7, .3737, 8.6, 4.322, 9, 679.4646, .8''',

'''           5.81\t\t           7.00
           0.37\t\t           8.60
           4.32\t\t           9.00
         679.46\t\t           0.80
----------------\t----------------
TOTAL=   689.97\t\tTOTAL=    25.40'''
),
(
'''10 A$="##"
20 PRINT USING A$;12.34
30 ON ERROR GOTO 100
35 Z$ = ""
40 PRINT USING Z$;12.34
45 ON ERROR GOTO 0
50 PRINT USING W$;12.34
60 END
100 PRINT "Error";ERR;"en línea";ERL
110 RESUME NEXT''',

'''12
Error 29 en línea 40
Line 50. Error in PRINT USING format.'''
),
(
'''10 A$="##"
20 PRINT USING A$;12.34
30 ON ERROR GOTO 50
40 A==W
50 Z$ = ""
60 ON ERROR GOTO 0
70 PRINT USING W$;12.34
80 END''',

'''12
Error in error handler: Error in PRINT USING format.'''
),
(
'''10 FOR I=-8 TO 8
20 X=1+1/3 : Y=1 : J=I
30 FOR J=I TO -1 : X=X/10 : Y=Y/10 : NEXT J
40 FOR J=I TO 1 STEP -1 : X=X*10 : Y=Y*10 : NEXT J
50 PRINT X,Y
60 NEXT I''',

''' 1.3333333333333E-08\t 1E-08
 1.3333333333333E-07\t 1E-07
 1.3333333333333E-06\t 1E-06
 1.3333333333333E-05\t 1E-05
 0.00013333333333333\t 0.0001
 0.0013333333333333\t 0.001
 0.013333333333333\t 0.01
 0.13333333333333\t 0.1
 1.3333333333333\t 1
 13.333333333333\t 10
 133.33333333333\t 100
 1333.3333333333\t 1000
 13333.333333333\t 10000
 133333.33333333\t 100000
 1333333.3333333\t 1000000
 13333333.333333\t 10000000
 133333333.33333\t 100000000'''
),
(
'''10 A=10: FOR X=1 TO 20: A=A/PI: PRINT A: NEXT X''',

''' 3.1830988618379
 1.0132118364234
 0.32251534433199
 0.10265982254684
 0.032677636430534
 0.010401614732959
 0.0033109368017757
 0.0010539039165349
 0.00033546803572089
 0.00010678279226862
 3.399001845341E-05
 1.081935890529E-05
 3.4439089017244E-06
 1.0962302505352E-06
 3.489409262791E-07
 1.1107134652877E-07
 3.5355107671852E-08
 1.1253880299043E-08
 3.5822213571144E-09
 1.1402564724682E-09'''
),
(
'''10 N=18
20 X=1.0E-4/3
30 FOR I=1 TO N
40 X=X/10
50 PRINT I,X;SPC(4);
60 X=X/10
70 PRINT X
80 NEXT I
90 END''',

''' 1\t 3.3333333333333E-06      3.3333333333333E-07
 2\t 3.3333333333333E-08      3.3333333333333E-09
 3\t 3.3333333333333E-10      3.3333333333333E-11
 4\t 3.3333333333333E-12      3.3333333333333E-13
 5\t 3.3333333333333E-14      3.3333333333333E-15
 6\t 3.3333333333333E-16      3.3333333333333E-17
 7\t 3.3333333333333E-18      3.3333333333333E-19
 8\t 3.3333333333333E-20      3.3333333333333E-21
 9\t 3.3333333333333E-22      3.3333333333333E-23
 10\t 3.3333333333333E-24      3.3333333333333E-25
 11\t 3.3333333333333E-26      3.3333333333333E-27
 12\t 3.3333333333333E-28      3.3333333333333E-29
 13\t 3.3333333333333E-30      3.3333333333333E-31
 14\t 3.3333333333333E-32      3.3333333333333E-33
 15\t 3.3333333333333E-34      3.3333333333333E-35
 16\t 3.3333333333333E-36      3.3333333333333E-37
 17\t 3.3333333333333E-38      3.3333333333333E-39
 18\t 3.3333333333333E-40      3.3333333333333E-41'''
 ),
 (
'''10 S=10^300
20 X=1.0E-4/3/S
30 FOR I=1 TO 10
40 X=X/10
50 PRINT I,X,
60 X=X/10
70 PRINT X
80 NEXT I
90 END''',

''' 1\t 3.3333333333333E-306\t 3.3333333333333E-307
 2\t 3.3333333333333E-308\t 3.3333333333333E-309
 3\t 3.3333333333334E-310\t 3.3333333333332E-311
 4\t 3.3333333333332E-312\t 3.3333333333282E-313
 5\t 3.3333333333776E-314\t 3.3333333348598E-315
 6\t 3.3333333447411E-316\t 3.3333334435543E-317
 7\t 3.333332455423E-318\t 3.3333126927971E-319
 8\t 3.3334609124909E-320\t 3.3349431094284E-321
 9\t 3.3596463917205E-322\t 3.4584595208887E-323
 10\t 4.9406564584125E-324\t 0'''
),
( 
'''10 S=10^300
20 X=1.0E-4/3/S
30 FOR I=1 TO 19
40 X=X/10
50 PRINT I,X,SIN(X)
80 NEXT I
90 END''',

''' 1\t 3.3333333333333E-306\t 3.3333333333333E-306
 2\t 3.3333333333333E-307\t 3.3333333333333E-307
 3\t 3.3333333333333E-308\t 3.3333333333333E-308
 4\t 3.3333333333333E-309\t 3.3333333333333E-309
 5\t 3.3333333333334E-310\t 3.3333333333334E-310
 6\t 3.3333333333332E-311\t 3.3333333333332E-311
 7\t 3.3333333333332E-312\t 3.3333333333332E-312
 8\t 3.3333333333282E-313\t 3.3333333333282E-313
 9\t 3.3333333333776E-314\t 3.3333333333776E-314
 10\t 3.3333333348598E-315\t 3.3333333348598E-315
 11\t 3.3333333447411E-316\t 3.3333333447411E-316
 12\t 3.3333334435543E-317\t 3.3333334435543E-317
 13\t 3.333332455423E-318\t 3.333332455423E-318
 14\t 3.3333126927971E-319\t 3.3333126927971E-319
 15\t 3.3334609124909E-320\t 3.3334609124909E-320
 16\t 3.3349431094284E-321\t 3.3349431094284E-321
 17\t 3.3596463917205E-322\t 3.3596463917205E-322
 18\t 3.4584595208887E-323\t 3.4584595208887E-323
 19\t 4.9406564584125E-324\t 4.9406564584125E-324'''
),
(
'''10 X=1
20 ON X GOSUB 100, 200, 300 : PRINT "!!!!"
30 END
100 PRINT "Hola"
110 RETURN''',

'''Hola
!!!!'''
),
(
'''10 X=4
20 ON X GOSUB 100, 200, 300 : PRINT "!!!!"
30 END
100 PRINT "Hola"
110 RETURN''',

'''!!!!'''
),
(
'''10 X=4                                                                                                                                                                                                                                                                                                                                                                                           
20 ON X GOTO 100, 200, 300: PRINT "!!!!"                                                                                                                                                                                                                                                                                                                                                                  
30 END                                                                                                                                                                                                                                                                                                                                                                                           
100 PRINT "Hola"                                                                                                                                                                                                                                                                                                                                                                                 
110 GOTO 30''',                                                                                                                                                                                                                                                                                                                                                                                   
                                                                                                                                                                                                                                                                                                                                                                                             
'''!!!!'''
),
( 
'''10 N=5                                                                                          
20 FOR I=1 TO 5                                                                                  
30 IF I=3 THEN 50                                                                                
40 NEXT I                                                                                        
50 PRINT "At line 50, i =";I                                                                     
60 FOR J=1 TO 2                                                                                  
70 FOR I=1 TO N                                                                                  
80 PRINT I,J                                                                                     
90 NEXT I                                                                                        
100 NEXT J                                                                                       
110 END''',

'''At line 50, i = 3
 1\t 1
 2\t 1
 3\t 1
 4\t 1
 5\t 1
 1\t 2
 2\t 2
 3\t 2
 4\t 2
 5\t 2'''
),
(
'''10 PRINT SPC(8)
20 PRINT "x";SPC(8),"y"
30 PRINT "x"+SPC(8),"y"
40 X$= "x"+TAB(8)+"y"
50 X$= TAB(8)
60 X$= "x";TAB(8);"y"
''',

'''        
x        \ty
Line 30. Expression not allowed.'''
),
(
'''10 PRINT 4.7\\3
20 PRINT -2.3\\1
30 PRINT INT(-2.3)
40 PRINT FIX(-2.3)
50 PRINT INT(2.3)
60 PRINT FIX(2.3)
70 PRINT FRAC(-2.3)
80 PRINT FRAC(2.3)
90 PRINT -2^-3.5''',

''' 1
-3
-3
-2
 2
 2
-0.3
 0.3
-0.088388347648318'''
),
(
'''10 REM Prueba
20 FOR I=0 TO 1 STEP 0.1
30 LET W=I*I
40 IF W>.5 THEN GOTO 70
50 PRINT I,W,I+((I+W)*I)
60 NEXT I
70 PRINT "Se acabó"
80 END''',

''' 0\t 0\t 0
 0.1\t 0.01\t 0.111
 0.2\t 0.04\t 0.248
 0.3\t 0.09\t 0.417
 0.4\t 0.16\t 0.624
 0.5\t 0.25\t 0.875
 0.6\t 0.36\t 1.176
 0.7\t 0.49\t 1.533
Se acabó'''
),
(
'''10 DIM A(10,10)
20 DIM B(10,10)
25 DIM C(10,10)
30 A(4,7) = 999
40 B(3,2) = 4 : B(9,6) = 7
55 C(1,1) = 3 : C(5,5) = 2
60 PRINT A(B( C( 1,  1),C(  5,5)),B( 9,   6 ))
70 PRINT A(B(3,C(5,5)),B(9,6))
80 PRINT A(B((9-C(1,1))/2,C(5,5)),B(9,6))''',

''' 999
 999
 999'''    
),
(
'''10 DIM A1$(10)
15 C$="5"
20 A1$(5)=B$="a"
25 PRINT A1$(C$)
30 PRINT A1$(B$)''',
                                                                                                
'''Line 25. Invalid value type.'''    
),
(
'''10 DIM A1$(10)
20 A1$(5)=B$="a"
30 PRINT A1$((10-5)''',
                                                                                               
'''Line 30. Syntax error.'''   
),
(
'''10 DIM A(10)
20 A(1)=7
30 Z=4:K=3
40 A(1)=11
50 A(2)=9
60 A(K)=A(Z-3)+A(3-1)
70 PRINT A(1),A(2),A(3)
80 END''',

''' 11\t 9\t 20'''
),
(
'''10 DIM A(10,10,10)
20 A(1,1,1)=7
30 Z=4 : K=3 : W=7
40 A(2,K,Z)=11
50 A(Z,6,K)=9
60 A(K,Z,W)=A(K-1,(Z+1)*2-W,Z)+A(3+1,((W-1)),K)
70 PRINT A(K,Z,W)
80 A(K-1,(Z+1)*2-W,Z)=A(3+1,(W-1),K)^2
90 PRINT A(2,3,4)
100 A(Z,((K)),W)=39
110 PRINT A(4,3,7)''',
                                                                                                                                                                                                                                                                        
''' 20
 81
 39'''  
),
(
'''10 x$="¡Hola, mundo!"
15 a$="," : b=5
20 PRINT INSTR(x$,a$)
30 PRINT INSTR(1,x$,",")
40 PRINT INSTR(b,x$,",")
50 PRINT INSTR(7,x$,",")
60 PRINT INSTR(x$,"!!")
65 PRINT INSTR(x$,"!")
70 PRINT INSTR(25,x$,",")
80 PRINT INSTR(0,x$,",")''',

''' 6
 6
 6
 0
 0
 13
 0
Line 80. Invalid argument.'''    
),
(
'''10 'Modifica la dirección
20 registro$="n: JUAN MORILLAS dir: ARTES 36 SALAMANCA"
30 PRINT "registro$ =            ";registro$
40 desplazam=INSTR(registro$,"dir:") 'Busca el principio de la dirección
50 MID$(registro$,desplazam,18)="dir: OLMO 22 YECLA, MURCIA  "
60 PRINT "registro$ modificado = ";registro$
70 MID$(registro$,desplazam)="dir: OLMO 22 YECLA, MURCIA"
80 PRINT "registro$ modificado = ";registro$''',

'''registro$ =            n: JUAN MORILLAS dir: ARTES 36 SALAMANCA
registro$ modificado = n: JUAN MORILLAS dir: OLMO 22 YECLAMANCA
registro$ modificado = n: JUAN MORILLAS dir: OLMO 22 YECLA, MUR'''    
),
(
'''
PRINT "AA"<"AB"
PRINT "X&">"X#"
PRINT "kg">"KG"
PRINT "QL ">"QL"
PRINT "ANA"<"ANABEL"
PRINT "06/09/1970">"03/12/1972"''',
'''-1
-1
-1
-1
-1
-1'''
),
(
r'''20 A$(1)="hola gran\\ de \n\t mundo"
30 IF A$(1)<>"Hola, mundo" THEN PRINT A$(1);
40 PRINT "\n"
50 PRINT """hola"""
60 X$="hola gran\\ de \n\t mundo"
70 IF X$<>"Hola, mundo" THEN PRINT X$;
80 PRINT "\n"
90 PRINT INSTR(X$,"\n")
100 PRINT MID$(X$,INSTR(X$,"\n"),2)
110 PRINT LEN("\ho\nla")
120 X$="\ho\nla" : PRINT LEN(X$)
130 A$(1)="hola" : A$(2)="hola\" : A$(3)="\hola"
140 A$(4)="\" : A$(5)="\\" : A$(6)="\ho\nla"
150 FOR X=1 TO 6 : PRINT A$(X);LEN(A$(X)) : NEXT
160 PRINT "hola\"; LEN("hola\")
170 PRINT "ho\nla"; LEN("ho\nla")
180 PRINT "\"; LEN("\")
190 PRINT UPPER$("ho\nla")
200 PRINT LOWER$("HO\NLA")
210 PRINT ASC("\")
220 PRINT STRING$(10,"\n")
230 DATA "\", ho\nla, \\, "\\", \, "ho\nla", "\a", "34"
240 FOR X=1 TO 8 : READ A$ : PRINT A$ : NEXT''',

r'''hola gran\\ de \n\t mundo\n
hola
hola gran\\ de \n\t mundo\n
 16
\n
 7
 7
hola 4
hola\ 5
\hola 5
\ 1
\\ 2
\ho\nla 7
hola\ 5
ho\nla 6
\ 1
HO\NLA
ho\nla
 92
\\\\\\\\\\
\
ho\nla
\\
\\
\
ho\nla
\a
34'''
),
(
'''10 DIM A(10,10,10)                                                                                                                                                                                                                                                         
20 Z=4 : K=3 : W=7                                                                                                                                                                                                                                                         
25 A(4,6,3)=39                                                                                                                                                                                                                                                             
26 PRINT A(3+1,((W-1)),K)                                                                                                                                                                                                                                                  
30 A(K-1,(Z+1)*2-W,Z)=A(3+1,((W-1)),K)                                                                                                                                                                                                                                     
40 PRINT A(2,3,4)''',

''' 39
 39''' 
),
(
'''10 DIM A(10)                                                                                                                                                                                                                                                               
20 A(3)+A(4)=8''',

'''Line 20. Syntax error.'''
),
(
'''10 DIM a1$(10,10,10)
20 Z=4 : K=3 : W=7
30 A1$(K-1,(Z+1)*2-W,Z)="A(3+1,((W-1)),K)"
40 PRINT A1$(2,3,4)''',

'''A(3+1,((W-1)),K)'''
),
(
'''10 REM Programa BASIC de prueba para evaluacion de arrays
20 DIM A(5), Y(5), B(5), C(5), Z9$(5,5), K$(5), A$(10), X$(10)
30 A(1) = 42
40 Y(2) = 3
50 C(4) = 2
60 B(2) = 1
70 B(3) = 4
80 Z9$(3,5) = "Hola"
90 A$ = "Mundo"
100 K$(5) = "Prueba"
110 X$(6) = "Cadena"
200 REM Imprimir los resultados para verificar
210 PRINT "A(1): "; A(1)
220 PRINT "X$(Y(2)+3): "; X$(Y(2)+3)
230 PRINT "A(B(C(4))): "; A(B(C(4)))
240 PRINT "Z9$(3,5): "; Z9$(3,5)
250 PRINT "K$(LEN(A$)): "; K$(LEN(A$))''',

'''A(1):  42
X$(Y(2)+3): Cadena
A(B(C(4))):  42
Z9$(3,5): Hola
K$(LEN(A$)): Prueba'''
),
(
'''10 REM Programa BASIC de prueba para variables largas
20 DIM IF1(5), TO2(5), ON3(5), CC(5), IF0$(5,5), KK$(5), AA9$(10), XY$(10)
30 IF1(1) = 42
40 TO2(2) = 3
50 CC(4) = 2
60 ON3(2) = 1
70 ON3(3) = 4
80 IF0$(3,5) = "Hola"
90 IF1$ = "Mundo"
100 KK$(5) = "Prueba"
110 XY$(6) = "Cadena"
200 REM Imprimir los resultados para verificar
210 PRINT "IF1(1): "; IF1(1)
220 PRINT "XY$(TO2(2)+3): "; XY$(TO2(2)+3)
230 PRINT "IF1(ON3(CC(4))): "; IF1(ON3(CC(4)))
240 PRINT "IF0$(3,5): "; IF0$(3,5)
250 PRINT "KK$(LEN(IF1$)): "; KK$(LEN(IF1$))''',

'''IF1(1):  42
XY$(TO2(2)+3): Cadena
IF1(ON3(CC(4))):  42
IF0$(3,5): Hola
KK$(LEN(IF1$)): Prueba'''   
),
(
'''10 LET A=B=C=5.089
20 D=2*8
30 X1=X2=(A+B)/(C+D)
40 X3=X4=A+B/C+D
50 LET A$=K$="TERMINATE"
60 PRINT A;B;C;D
70 PRINT X1;X2
80 PRINT X3;X4;
90 PRINT A$;K$''',

''' 5.089  5.089  5.089  16
 0.48262127175305  0.48262127175305
 22.089  22.089 TERMINATETERMINATE'''  
),
(
'''10 X=12 : Y=-5
20 PRINT "x=";X,"y=";Y
30 A=12345678901234
40 B=1234567890123456
50 C=-0.001234
60 D=0.000012345
70 E=-1234.5
80 F=123456789012.2456
90 PRINT A,B,C
100 PRINT D,E,F''',

'''x= 12\ty=-5
 12345678901234\t 1.2345678901235E+15\t-0.001234
 1.2345E-05\t-1234.5\t 123456789012.25''' 
),
(
'''10 DEF FNX(A,B)=A^2+B^2
20 FOR S=1 TO 3
30 FOR T=1 TO 3
40 PRINT FNX(S,T),S,T
50 NEXT T
60 NEXT S''',

''' 2\t 1\t 1
 5\t 1\t 2
 10\t 1\t 3
 5\t 2\t 1
 8\t 2\t 2
 13\t 2\t 3
 10\t 3\t 1
 13\t 3\t 2
 18\t 3\t 3'''
),
(
'''10 DEF FNX(A$,B$,C)=A$+B$+STR$(C)
20 Z$=FNX("a","b",3)
30 PRINT Z$''',

'''ab3'''
),
(
'''10 DEF FNX$(A$,B$) = A$+"|"+B$
20 DEF FNX(A$,B$) = A$+"+"+B$
30 PRINT FNX$("hola","adios")
40 PRINT FNX("hola","adios")
50 DEF FNX$(A$,B$) = A$+"="+B$
60 PRINT FNX$("hola","adios")
70 PRINT FNX("hola","adios")''',

'''hola|adios
hola+adios
hola=adios
hola+adios'''    
),
(
'''10 PRINT LEFT$("Hola Mundo", 4)
20 GOSUB 230
30 LET A$ = "Hola Mundo"
40 PRINT LEFT$(A$, 4)
50 GOSUB 230
60 DIM B$(10)
70 LET Z = 1
80 LET B$(Z) = "Hola Mundo"
90 PRINT LEFT$(B$(2-Z), 4)
100 GOSUB 230
110 DIM A(10)
120 LET A(1) = 7.5
130 PRINT INT(A(1))
140 GOSUB 230
150 DIM X(3,3)
160 FOR X=1 TO 3 STEP 1
170 FOR Y=1 TO 3
180 LET X(X,Y)=X*Y
190 PRINT X,Y,X(X,Y)
200 NEXT Y
210 NEXT X
220 STOP
230 PRINT "SUBRUTINA!!"
240 RETURN
250 END''',

'''Hola
SUBRUTINA!!
Hola
SUBRUTINA!!
Hola
SUBRUTINA!!
 7
SUBRUTINA!!
 1\t 1\t 1
 1\t 2\t 2
 1\t 3\t 3
 2\t 1\t 2
 2\t 2\t 4
 2\t 3\t 6
 3\t 1\t 3
 3\t 2\t 6
 3\t 3\t 9'''
),
(
'''10 FOR X=1 TO 10
20 LET Z=8: IF X=5 THEN LET A=0: LET X=10 ELSE LET A=X*2
30 PRINT A,X,Z
40 NEXT X
50 END''',

''' 2\t 1\t 8
 4\t 2\t 8
 6\t 3\t 8
 8\t 4\t 8
 0\t 10\t 8'''
),
(
'''10 A=4
20 FOR X=1 TO 10
30 IF X=5 THEN IF A=6 THEN A=7:X=10 ELSE A=9
40 PRINT A,X
50 NEXT X
60 END''',

''' 4\t 1
 4\t 2
 4\t 3
 4\t 4
 9\t 5
 9\t 6
 9\t 7
 9\t 8
 9\t 9
 9\t 10'''
),
(
'''10 A=4
20 FOR X=1 TO 10
30 IF X=5 THEN IF A=6 THEN A=7:X=10 ELSE A=12 ELSE A=9
40 PRINT A,X
50 NEXT X
60 END''',

''' 9\t 1
 9\t 2
 9\t 3
 9\t 4
 12\t 5
 9\t 6
 9\t 7
 9\t 8
 9\t 9
 9\t 10'''
),
(
'''10 X=3 : IF X=10 THEN PRINT "1":IF X=3 THEN IF X=6 THEN PRINT "2" ELSE PRINT "3"''',

''''''
),
(
'''10 X=10 : IF X=10 THEN PRINT "1":IF X=3 THEN IF X=6 THEN PRINT "2" ELSE PRINT "3"''',

'''1'''
),
(
'''10 X=10 : IF X=10 THEN PRINT "1":IF X>5 THEN IF X=6 THEN PRINT "2" ELSE PRINT "3"''',           
                                                                                       
'''1
3'''    
),
(
'''10 X=10 : IF X=10 THEN PRINT "1":IF X>5 THEN IF X>6 THEN PRINT "2" ELSE PRINT "3"''',           
                                                                                         
'''1
2'''    
),
(
'''10 X=3
20 IF X=10 THEN GOSUB 40:PRINT "77" ELSE GOSUB 50:PRINT "88"
30 END
40 PRINT "hola" : RETURN
50 PRINT "adios" : RETURN''',

'''adios
88'''    
),
(
'''10 A$= "  hola  "
20 B$="ADIOS"
30 PRINT A$+B$
40 PRINT TRIM$(A$)+B$
50 PRINT TRIM$(UPPER$(A$))+LOWER$(B$)
60 PRINT TRIM$(A$)+LOWER$(B$)
70 B$="  hola  5"
80 PRINT RIGHT$(B$,1)
90 A=7:B$="hola  a"
100 PRINT B$
110 PRINT RIGHT$(B$,1)
120 END''',

'''  hola  ADIOS
holaADIOS
HOLAadios
holaadios
5
hola  a
a'''
),
(
'''10 READ A$
15 PRINT LEN(A$);
20 FOR I=1 TO LEN(A$)
25 PRINT ASC(MID$(A$,I,1));
30 NEXT I
40 DATA ABCDEFGHIJKLMNOPQRSTUVWXYZ
45 END''',

''' 26  65  66  67  68  69  70  71  72  73  74  75  76  77  78  79  80  81  82  83  84  85  86  87  88  89  90 '''
),
(
'''10 DATA Antes, 1e6, 8, Cebra, "hola ",  adios
20 DIM B$(10)
30 DIM A$(2,2)
40 READ B$(1),A,B,A$(1,1),C$,D$
50 PRINT B$(1),A,A+1,B+2,A$(1,1),C$;D$
60 END''',

'''Antes\t 1000000\t 1000001\t 10\tCebra\thola adios'''
),
(
'''10 X=0
20 WHILE NOT(X=5)
30 X=X+1
40 PRINT X
50 WEND''',

''' 1
 2
 3
 4
 5'''
),
(
'''10 X=0 : WHILE X<5 : X=X+1 : PRINT X : WEND''',

''' 1
 2
 3
 4
 5'''
),
(
'''10 X=0 : WHILE X<>5 : X=X+1
20 PRINT X : WEND''',

''' 1
 2
 3
 4
 5'''
),
(
'''10 X=0 : Y =0 : WHILE Y<5 : WHILE X<5 : X=X+1
20 Y=Y+1 : WEND : PRINT X,Y : WEND''',

''' 5\t 5'''
),
(
'''10 X=0 : Y=0 : WHILE Y<2 : WHILE X<2: W=7
20 X=X+1 :PRINT "X=";X : WEND : Y=Y+1 : PRINT "Y=";Y : WEND''',

'''X= 1
X= 2
Y= 1
Y= 2'''
),
(
'''10 C=0
20 WHILE C<20
30 WHILE C<1
40 FOR X=1 TO 10
50 C=C+1
60 NEXT X
70 WEND
75 C=C+1
80 WEND
90 PRINT "Hola"''',

'''Hola'''    
),
(
'''10 X=0
20 X=X+1 : PRINT X : IF X<5 GOTO 20''',

''' 1
 2
 3
 4
 5'''
),
(
'''10 FOR X=1 TO 5 : PRINT X : NEXT
20 FOR X=1 TO 5 : PRINT Y : NEXT''',

''' 1
 2
 3
 4
 5
 0
 0
 0
 0
 0'''    
),
(
'''10 FOR Y=1 TO 5 : FOR X=1 TO 5
20 Z=7 : NEXT : W=Z : PRINT X,Y : NEXT''',

''' 6\t 1
 6\t 2
 6\t 3
 6\t 4
 6\t 5'''
),
(
'''10 FOR Y=1 TO 5 : FOR X=1 TO 5
20 PRINT X;Y : NEXT : NEXT''',

''' 1  1
 2  1
 3  1
 4  1
 5  1
 1  2
 2  2
 3  2
 4  2
 5  2
 1  3
 2  3
 3  3
 4  3
 5  3
 1  4
 2  4
 3  4
 4  4
 5  4
 1  5
 2  5
 3  5
 4  5
 5  5'''
),
(
'''10 FOR Y=1 TO 5 : FOR X=1 TO 5
20 NEXT : PRINT X;Y : NEXT''',

''' 6  1
 6  2
 6  3
 6  4
 6  5'''
),
(
'''10 FOR Y=1 TO 2 : FOR X=1 TO 2
20 PRINT "X =";X : NEXT : PRINT "Y =";Y : NEXT''',

'''X = 1
X = 2
Y = 1
X = 1
X = 2
Y = 2'''
),
(
'''10 FOR X=1 TO 2
20 PRINT X : NEXT''',

''' 1
 2'''
),
(
'''10 X=0 : FOR X=1 TO 5 : Y=0
20 PRINT X : NEXT''',

''' 1
 2
 3
 4
 5'''
),
(
'''10 FOR X=1 TO 10
20 PRINT X
30 FOR Y=1 TO 5
40 PRINT Y
50 NEXT X
60 PRINT X,Y
70 NEXT Y''',

''' 1
 1
 2
 1
 3
 1
 4
 1
 5
 1
 6
 1
 7
 1
 8
 1
 9
 1
 10
 1
 11\t 1
Line 70. NEXT without matching FOR.'''
),
(
'''10 FOR X=1 TO 2
20 FOR Y=1 TO 2
30 PRINT X,Y
40 NEXT Y
50 NEXT X
60 END''',

''' 1\t 1
 1\t 2
 2\t 1
 2\t 2'''
),
(
'''10 FOR X=1 TO 2: FOR Y=1 TO 2: PRINT X,Y: NEXT Y: NEXT X''',

''' 1\t 1
 1\t 2
 2\t 1
 2\t 2'''
),
(
'''10 FOR X=1 TO 2
20 FOR Y=1 TO 2
30 FOR Z=1 TO 2
40 PRINT X,Y,Z
50 NEXT Z
60 NEXT Y
70 NEXT X
80 END''',

''' 1\t 1\t 1
 1\t 1\t 2
 1\t 2\t 1
 1\t 2\t 2
 2\t 1\t 1
 2\t 1\t 2
 2\t 2\t 1
 2\t 2\t 2'''
),
(
'''10 FOR X=1 TO 2: FOR Y=1 TO 2: FOR Z=1 TO 2: PRINT X,Y,Z: NEXT Z: NEXT Y: NEXT X''',

''' 1\t 1\t 1
 1\t 1\t 2
 1\t 2\t 1
 1\t 2\t 2
 2\t 1\t 1
 2\t 1\t 2
 2\t 2\t 1
 2\t 2\t 2'''
),
(
'''10 FOR X=1 TO 2: FOR Y=1 TO 2: FOR Z=1 TO 2: PRINT X,Y,Z: NEXT: NEXT: NEXT Z''',

''' 1\t 1\t 1
 1\t 1\t 2
 1\t 2\t 1
 1\t 2\t 2
Line 10. NEXT without matching FOR.'''
),
(
'''10 PRINT "Comienzo":FOR X=1 TO 5:PRINT X:NEXT X''',

'''Comienzo
 1
 2
 3
 4
 5'''
),
(
'''10 FOR X=1 TO 5 : PRINT "*"
20 PRINT X
30 NEXT X''',

'''*
 1
*
 2
*
 3
*
 4
*
 5'''
),
(
'''10 Z=1
20 X=1:GOSUB 60
30 PRINT X
40 Z=Z+1:IF Z=5 THEN STOP
50 GOTO 20
60 PRINT X:X=99:RETURN''',

''' 1
 99
 1
 99
 1
 99
 1
 99'''
),
(
'''10 X=1:GOSUB 20
20 PRINT X:X=7:RETURN''',

''' 1
 7
Line 20. RETURN without matching GOSUB.'''
),
(
'''10 X=1:GOSUB 20
15 ON ERROR GOTO 30
20 PRINT X:X=7:RETURN
30 PRINT "adios"''',

''' 1
 7
adios'''
),
(
'''10 X=1:GOSUB 20
20 PRINT X:X=7:RETURN
25 ON ERROR GOTO 30
30 PRINT "adios"''',

''' 1
 7
Line 20. RETURN without matching GOSUB.'''
),
(
'''10 X=0: Y=3
20 IF X=0 THEN IF Y=0 THEN PRINT "x=0 y=0" ELSE IF Y=1 THEN PRINT "x=0 y=1" ELSE PRINT "x=0 y=?"''',

'''x=0 y=?'''
),(
'''10 X=5: GOSUB 30
20 PRINT "FIN":STOP
30 PRINT X: GOSUB 60
40 PRINT X*X*X
50 RETURN
60 PRINT X*X: RETURN
70 END''',

''' 5
 25
 125
FIN'''
),
(
'''20 DIM A(-3)''',

'''Line 20. Invalid value.'''
),
(
'''30 DIM B(,3)''',

'''Line 30. Undefined index.'''
),
(
'''40 DIM C(3/0)''',

'''Line 40. Division by zero.'''
),
(
'''50 RETURN''',

'''Line 50. RETURN without matching GOSUB.'''
),
(
'''60 FOR X=1 TO 7
70 NEXT Y0''',

'''Line 70. NEXT without matching FOR.'''
),
(
'''70 NEXT Y''',

'''Line 70. NEXT without matching FOR.'''
),
(
'''80 X=4+"w"''',

'''Line 80. Invalid value type.'''
),
(
'''10 X$ = A1$(18) + "hola"''',

'''Line 10. Index out of range.'''
),
(
'''10 A$="C$"
20 A=8
30 C$="A"
40 PRINT A$
50 X$ = "B$" + "hola" : PRINT X$
60 X$ = B$ + "hola"''',

'''C$
B$hola'''
),
(
'''90 X=''',

'''Line 90. Syntax error.'''
),
(
'''100 A="hola"''',

'''Line 100. Invalid value type.'''
),
(
'''110 X=3/0''',

'''Line 110. Division by zero.'''
),
(
'''120 RINT 2''',

'''Line 120. Syntax error.'''
),
(
'''130 X = FNX(5)''',

'''Line 130. Undefined variable or function.'''
),
(
'''140 DEF FNA()''',

'''Line 140. Malformed function.'''
),
(
'''150 GOTO 85''',

'''Line 150. Target line does not exist.'''
),
(
'''10 DIM A(10)
20 A(7)=A(8)=9
30 A(7)=10
40 SWAP A(7),A(8)
50 PRINT A(7),A(8)
60 DIM B(5)
70 B(4)=B(7)=10''',

''' 9\t 10
Line 70. Index out of range.'''
),
(
'''5 ON ERROR GOTO 100
10 DIM A$(10)
20 A$(7)=A$(8)="hola"
30 A$(7)="adios"
40 SWAP A(17),A(18) : PRINT A(17)
45 PRINT A(18)
50 END
100 PRINT "la monda"
110 PRINT ERR; ERL
120 RESUME NEXT''',

'''la monda
 34  40
la monda
 34  40
la monda
 34  45'''
),
(
'''5 ON ERROR GOTO 100
40 SWAP A(17),A(18) : PRINT A(17)
50 END
100 PRINT "hola"
110 RESUME NEXT''',

'''hola
hola'''    
),
(
'''10 DIM X(10)
20 X(7)=5
30 X(6)=X((4+3))
40 PRINT X(6)''',
''' 5'''  
),
(
'''10 DIM A(10,10)
20 A(7,5)=A(8,2)=9
30 PRINT A(7,5),A(8,2)
40 A(3,3)=A((4*(1+1)),3^2-(4*2)+1)*2
50 PRINT A(3,3)  ''',

''' 9\t 9
 18'''
),
(
'''10 DIM A$(10)
20 A$(7)=A$(8)="hola"
30 A$(7)="adios"
40 SWAP A$(7),A$(8)
50 PRINT A$(7),A$(8)
60 DIM Z(10)
70 Z(6)=V=25
80 PRINT Z(6);V
90 V=19
100 PRINT Z(6);V
110 SWAP V,Z(6)
120 PRINT Z(6);V
130 SWAP Z(6),V
140 PRINT Z(6);V''',

'''hola\tadios
 25  25
 25  19
 19  25
 25  19'''  
),
(
'''10 A=B=7
20 B=2
30 SWAP A,B
40 PRINT A,B
50 A$=B$="hola"
60 A$="adios"
70 SWAP A$,B$
80 PRINT A$,B$
90 A=B$=5''',

''' 2\t 7
hola\tadios
Line 90. Invalid value type.'''
),
(
'''10 X=7
20 Y=2
30 SWAP X,Y+1
40 PRINT X,Y''',

'''Line 30. Invalid argument.'''
),
(
'''10 DIM A(2,2),B(2,2)
20 A(1,1)=10
30 B(1,1)=20
40 SWAP A,B
50 PRINT A(1,1),B(1,1)''',

'''Line 40. Invalid argument.'''
),
(
'''10 DIM A$(10)
20 A$(7)=A$(8)="hola"
30 A$(7)="adios"
40 SWAP A(17),A(18)
50 PRINT A(17),A(18)''',

'''Line 40. Index out of range.'''  
),
(
'''10 DIM A$(10)
20 A$(7)=A$(8)="hola"
30 A$(7)="adios"
40 SWAP A(7),A(8)
50 PRINT A(7),A(8)''',

''' 0\t 0'''  
),
(
'''5 ON ERROR GOTO 100
10 DIM A$(10)
20 A$(7)=A$(8)="hola"
30 A$(7)="adios"
40 SWAP A(7),A(8) : PRINT A(7)
45 PRINT A(8)
50 END
100 PRINT "la monda"
110 PRINT ERR; ERL
120 RESUME NEXT''',

''' 0
 0'''
),
(
'''5 ON ERROR GOTO 100
40 SWAP A(7),A(8) : PRINT A(7)
50 END
100 PRINT "hola"
110 RESUME NEXT''',

''' 0'''    
),
(
'''100 ON ERROR GOTO 230
110 w=k(5)-2
120 PRINT w
130 IF w=-2 THEN k(12)=7
140 MID$(a$,3,2)="XX"
150 a$="Amstrad"
160 MID$(a$,3,20)="XX"
170 MID$(a$,0,20)="XX"
180 MID$(a$,3,-1)="XX"
190 ON ERROR GOTO 0
200 z=7/t(5)
210 END
220 '8:UNDEFINED, 35:INDEX_OUT_OF_BOUNDS, 36:OUT_OF_BOUNDS
230 PRINT ERL,ERR
240 RESUME NEXT''',

'''-2
 130\t 34
 170\t 35
 180\t 35
Line 200. Division by zero.'''
),
(
'''10 N=3
20 FOR F=1 TO 2
30 FOR C=1 TO N : A(F,C) = F*C
40 PRINT A(F,C)
50 NEXT C
60 NEXT F
70 END''',

''' 1
 2
 3
 2
 4
 6'''
),
(
'''10 N=3
20 DIM D(2,N), P(2,2)
30 FOR F=1 TO 2
40 FOR C=1 TO N : PRINT F*C : NEXT C
50 NEXT F
60 END''',

''' 1
 2
 3
 2
 4
 6'''
),
(
'''10 N=3
20 DIM D(2,N), P(2,2)
30 FOR F=1 TO 2
40 FOR C=1 TO N
50 PRINT F*C
60 NEXT C
70 NEXT F
80 END''',

''' 1
 2
 3
 2
 4
 6'''
),
(
'''10 FOR F=1 TO 2 : M=0 : V=0
20 FOR C=1 TO 3: M=M+8: PRINT M : NEXT C
30 PRINT M/3
40 FOR C=1 TO 2 : V=V+(ABS(6-3))^2 : NEXT C
50 PRINT V/2
60 NEXT F
70 END''',

''' 8
 16
 24
 8
 9
 8
 16
 24
 8
 9'''
),
(
'''10 FOR X=49 TO 55:PRINT CHR$(X);:IF X MOD 50=0 THEN PRINT:NEXT X: PRINT''',

'''1'''
),
(
'''10 FOR X=50 TO 55:PRINT CHR$(X);:IF X MOD 50=0 THEN PRINT:NEXT X: PRINT "PERA"''',

'''2
3'''
),
(
'''10 X=INT(65.2)
20 PRINT CHR$(X)
30 PRINT CHR$(INT(65.2))
40 END''',

'''A
A'''
),
(
'''10 PRINT CHR$("65")''',

'''Line 10. Invalid value type.'''
),
(
'''PRINT CHR$(34)+"HOLA"+CHR$(34)''',

'''"HOLA"'''
),
(
'''10 FOR X=1 TO 5 : IF X<10 THEN PRINT X:NEXT:PRINT "pera"''',

''' 1
 2
 3
 4
 5
pera'''  
),
(
'''10 FOR X=1 TO 5:PRINT "hola";:IF X<10 THEN PRINT " mundo":NEXT X: PRINT "adios"''',

'''hola mundo
hola mundo
hola mundo
hola mundo
hola mundo
adios'''
),
(
'''10 A$="#" : B$="-" : X$=""
20 FOR X=1 TO 10
30 A$=A$+"#"
35 Z$=A$+B$
40 X$=X$+Z$
50 PRINT X$
60 NEXT''',

'''##-
##-###-
##-###-####-
##-###-####-#####-
##-###-####-#####-######-
##-###-####-#####-######-#######-
##-###-####-#####-######-#######-########-
##-###-####-#####-######-#######-########-#########-
##-###-####-#####-######-#######-########-#########-##########-
##-###-####-#####-######-#######-########-#########-##########-###########-'''    
),
(
'''10 FOR X=49 TO 55
20 PRINT CHR$(X);
30 IF X MOD 50=0 THEN PRINT
40 NEXT X
50 END''',

'''12
34567'''
),
(
'''10 FOR X=1 TO 100000000
20 PRINT X
30 IF X=10 THEN STOP
40 NEXT X
50 END''',

''' 1
 2
 3
 4
 5
 6
 7
 8
 9
 10'''
),
(
'''5 X=0
10 FOR X=1 TO 2
20 FOR Y=1 TO 2
30 FOR Z=1 TO 2
40 PRINT X,Y,Z
45 IF X=2 AND Y=2 AND Z=2 THEN GOTO 80
50 NEXT Z
60 NEXT Y
70 NEXT X
75 STOP
80 PRINT "Hola"
90 GOTO 70''',

''' 1\t 1\t 1
 1\t 1\t 2
 1\t 2\t 1
 1\t 2\t 2
 2\t 1\t 1
 2\t 1\t 2
 2\t 2\t 1
 2\t 2\t 2
Hola'''
),
(
'''10 PRINT "hola": GOTO 50: PRINT "adios"
20 PRINT "terminado"
30 STOP
50 PRINT "empezado"
60 GOTO 20
70 END''',

'''hola
empezado
terminado'''
),
(
'''10 PRINT "hola": X=7: GOSUB 50: PRINT "adios": PRINT X
20 PRINT "terminado"
30 STOP
50 PRINT "empezado"
60 RETURN
70 END''',

'''hola
empezado
adios
 7
terminado'''
),
(
'''10 GOSUB 50
20 PRINT "terminado"
30 STOP
50 PRINT "empezado"
60 RETURN
70 END''',

'''empezado
terminado'''
),
(
'''10 FOR A=0 TO 10
20 IF A=1 THEN 40
30 PRINT A
35 GOTO 50
40 A=2:GOTO 20
50 NEXT A''',

''' 0
 2
 3
 4
 5
 6
 7
 8
 9
 10'''
),
(
'''10 FOR X=1 TO 1000
15 PRINT X;
20 IF X=10 THEN 40
30 NEXT X
35 PRINT "fin"
40 X=999
50 GOTO 15''',

''' 1  2  3  4  5  6  7  8  9  10  999  1000 fin
 999 
Line 30. NEXT without matching FOR.'''
),
(
'''10 N=100 : DIM A(N)
15 RANDOMIZE 1 'La misma secuencia siempre
20 M1=0
25 M2=0
30 T=TIME
35 FOR X=1 TO N
40 A(X)=RND
45 M2=MAX(M2+A(X),0)
50 M1=MAX(M1,M2)
55 NEXT
60 T=TIME-T
65 PRINT M1
70 'PRINT USING "#.##";T;" seg."''',

''' 51.237520131839'''  
),
(
'''1 S=7000
2 DIM F(S)
3 PRINT "Only 1 iteration"
4 'T=TIME
5 C=0
6 FOR I=1 TO S
7 F(I)=1
8 NEXT I
9 FOR I=0 TO S
10 IF F(I)=0 THEN 18
11 P=I+I+3
12 K=I+P
13 IF K>S THEN 17
14 F(K)=0
15 K=K+P
16 GOTO 13
17 C=C+1
18 NEXT I
19 PRINT C;"primes"
20 END''',

'''Only 1 iteration
 1651 primes'''
),
(
'''10 FOR X=1 TO 3
20 FOR Y=1 TO 3
30 PRINT X;Y
35 GOTO 50
40 NEXT Y
50 NEXT X''',

''' 1  1
 2  1
 3  1'''
),
(
'''10 FOR X=1 TO 10
20 FOR Y=1 TO 10
30 PRINT X;Y
40 GOTO 60
50 NEXT Y
60 NEXT X
70 GOTO 50''',

''' 1  1
 2  1
 3  1
 4  1
 5  1
 6  1
 7  1
 8  1
 9  1
 10  1
Line 50. NEXT without matching FOR.'''   
),
(
'''10 FOR X=1 TO 10
20 FOR Y=1 TO 10
30 PRINT X;Y
40 GOTO 60
50 NEXT Y
60 NEXT X
70 GOTO 20''',

''' 1  1
 2  1
 3  1
 4  1
 5  1
 6  1
 7  1
 8  1
 9  1
 10  1
 11  1
Line 60. NEXT without matching FOR.'''    
),
(
'''10 FOR X=1 TO 10
20 FOR Y=1 TO 10
30 PRINT X;Y
40 NEXT X
50 NEXT Y''',

''' 1  1
 2  1
 3  1
 4  1
 5  1
 6  1
 7  1
 8  1
 9  1
 10  1
Line 50. NEXT without matching FOR.'''  
),
(
'''10 X=Y=0
20 WHILE X<3
30 X=X+1
40 WHILE Y<3
50 Y=Y+1
60 PRINT X;Y
70 WEND
80 WEND''',

''' 1  1
 1  2
 1  3'''    
),
(
'''10 X=Y=0
20 WHILE X<3
30 X=X+1
40 WHILE Y<3
50 Y=Y+1
60 PRINT X;Y
70 WEND
75 Y=0
80 WEND''',

''' 1  1
 1  2
 1  3
 2  1
 2  2
 2  3
 3  1
 3  2
 3  3'''    
),
(
'''10 X=Y=0
20 WHILE X<3
30 X=X+1
40 WHILE Y<3
50 Y=Y+1
60 PRINT X;Y
65 GOTO 80
70 WEND
75 Y=0
80 WEND''',

''' 1  1
 1  2
 1  3'''    
),
(
'''10 ' Al contrario que las variantes de Microsoft BASIC, pero
20 ' como en Locomotive BASIC, los límites de los bucles FOR
30 ' se establecen al principio y no en cada iteración del bucle.
40 ' Este programa en Microsoft BASIC imprimiría del 1 al 15.
50 N=10
60 FOR X=1 TO N
70 PRINT X
80 IF X=5 THEN N=15
90 NEXT X''',

''' 1
 2
 3
 4
 5
 6
 7
 8
 9
 10'''
),
(
'''100 ' No hay EXIT FOR. Hay que salir del bucle con GOTO
110 FOR X=1 TO 5
120 FOR Y=1 TO 5
130 PRINT X;Y
140 IF Y=3 GOTO 160
150 NEXT Y
160 NEXT X
170 PRINT
180 ' Pero si hay bucles anidados hay que poner el nombre
190 ' de la variable en el NEXT ya que, si encuentra un
200 ' NEXT sin variable, supone que pertenece al último FOR
210 FOR X=1 TO 5
220 FOR Y=1 TO 5
230 PRINT X;Y
240 IF Y=3 GOTO 260
250 NEXT
260 NEXT''',

''' 1  1
 1  2
 1  3
 2  1
 2  2
 2  3
 3  1
 3  2
 3  3
 4  1
 4  2
 4  3
 5  1
 5  2
 5  3

 1  1
 1  2
 1  3
 1  4
 1  5
 2  1
 2  2
 2  3
 2  4
 2  5
 3  1
 3  2
 3  3
 3  4
 3  5
 4  1
 4  2
 4  3
 4  4
 4  5
 5  1
 5  2
 5  3
 5  4
 5  5'''    
),
(
'''10 IF0=7
20 TO3=5 : ON4=9
30 IF1$="555" : TO3$="444"
40 DIM IF6(10)
50 IF6(0)=4
60 FOR CN=1 TO 10 : IF6(CN)=CN : NEXT CN
70 FOR XX1=0 TO 10 : PRINT IF6(XX1) : NEXT XX1
80 PRINT TO3-ON4
90 IF IF0>5 THEN KK=(IF0+1)*2+ON4
95 ON4=KK*2
100 PRINT KK;ON4
110 PRINT VAL(IF1$)+VAL(TO3$)+1
120 PRINT "IF0="+STR$(IF0)+" TO3=";STR$(TO3)
130 PRINT SIN(COS(IF0))
140 XX1=2 : XX2=4
150 IF XX1<XX2 THEN PRINT XX1+XX2''',

''' 4
 1
 2
 3
 4
 5
 6
 7
 8
 9
 10
-4
 25  50
 1000
IF0=7 TO3=5
 0.68448879899261
 6'''  
),
(
'''10 DATA 11, 22, 33, 44, 55, 66, 77, 88, 99, 1010
15 DIM KK3(10)
20 FOR FF=1 TO 10
30 READ KK3(FF)
40 NEXT
50 FOR IF1=1 TO 10
60 PRINT KK3(IF1)/10
70 NEXT IF1''',

''' 1.1
 2.2
 3.3
 4.4
 5.5
 6.6
 7.7
 8.8
 9.9
 101'''  
),
(
'''10 DEF FNUN1(PE1)=PE1^2
20 DEF FNOT2$(PE1$)=PE1$+" "+PE1$
30 PR1= 2 : PRINT FNUN1(PR1)
40 PR2$= "hola" : PRINT FNOT2$(PR2$)''',

''' 4
hola hola'''
),
(
'''10 DEF FNUNACOSA(Pera1)=pERa1^2
20 DEF FNYOTRA$(Pera1$)=pERa1$+" "+PerA1$
30 pRUEba1= 2 : PRINT FNUNACOSA(Prueba1)
40 Prueba2$= "hola" : PRINT FNYOTRA$(prUEBa2$)''',

''' 4
hola hola'''
),
(
'''10 DIM L(15) ' Array para almacenar números automórficos
20 L(1) = 1 ' Inicializar la lista con el primer número automórfico
30 N = K = 1
40 L1 = 1E8 ' No hay precisión para números más grandes
50 WHILE K < L1
60 FOR P = 1 TO N
70 FOR D = 1 TO 9
80 M = D * K + L(P)
90 IF (M * M) MOD (K * 10) = M THEN N = N + 1: L(N) = M
100 NEXT D
110 NEXT P
120 K = K * 10
130 WEND
140 ' Ordenar L e imprimirlo
150 FOR I = 1 TO N-1
160 IF L(I) > L(I+1) THEN SWAP L(I), L(I+1)
170 PRINT L(I)
180 NEXT I
190 PRINT L(I)''',

''' 1
 5
 6
 25
 76
 376
 625
 9376
 90625
 109376
 890625
 2890625
 7109376
 12890625
 87109376'''    
),
(
'''10 REM GENERACIÓN DE NÚMEROS DE FIBONACCI Y BÚSQUEDA DE PRIMOS
20  'PRINT "N=";
30 N=30 'INPUT N
40 PRINT
50 PRINT "GENERACIÓN DE NÚMEROS DE FIBONACCI Y BÚSQUEDA DE PRIMOS"
60 PRINT
70 LET F1=1
80 LET F2=1
90 PRINT "I=";1,"F=";1;" (PRIMO)"
100 PRINT "I=";2,"F=";1;" (PRIMO)"
110 FOR I=3 TO N '(GENERAR NÚMERO DE FIBONACCI)
120   LET F=F1+F2
130   FOR J=2 TO INT(SQR(F)) '(PRUEBA DE NÚMEROS PRIMOS)
140     LET Q=F/J
150     LET Q1=INT(Q)
160     IF Q=Q1 THEN 200
170   NEXT J
180   PRINT "I=";I,"F=";F;" (PRIMO)"
190   GOTO 210
200   PRINT "I=";I,"F=";F
210   LET F2=F1
220   LET F1=F
230 NEXT I
240 END''',

'''
GENERACIÓN DE NÚMEROS DE FIBONACCI Y BÚSQUEDA DE PRIMOS

I= 1\tF= 1  (PRIMO)
I= 2\tF= 1  (PRIMO)
I= 3\tF= 2  (PRIMO)
I= 4\tF= 3  (PRIMO)
I= 5\tF= 5  (PRIMO)
I= 6\tF= 8
I= 7\tF= 13  (PRIMO)
I= 8\tF= 21
I= 9\tF= 34
I= 10\tF= 55
I= 11\tF= 89  (PRIMO)
I= 12\tF= 144
I= 13\tF= 233  (PRIMO)
I= 14\tF= 377
I= 15\tF= 610
I= 16\tF= 987
I= 17\tF= 1597  (PRIMO)
I= 18\tF= 2584
I= 19\tF= 4181
I= 20\tF= 6765
I= 21\tF= 10946
I= 22\tF= 17711
I= 23\tF= 28657  (PRIMO)
I= 24\tF= 46368
I= 25\tF= 75025
I= 26\tF= 121393
I= 27\tF= 196418
I= 28\tF= 317811
I= 29\tF= 514229  (PRIMO)
I= 30\tF= 832040'''
),
(
'''10 PRINT "NÚMEROS DE FIBONACCI"
20 N=30 : F1=0 : F2=1
30 FOR I=1 TO N
40 SWAP F1,F2 : F1=F1+F2
50 PRINT : PRINT "I=";I,"F=";F1;
60 FOR J=2 TO INT(SQR(F1))
70 IF F1 MOD J=0 THEN 100
80 NEXT J
90 PRINT " (PRIMO)";
100 NEXT I
110 END''',

'''NÚMEROS DE FIBONACCI

I= 1\tF= 1  (PRIMO)
I= 2\tF= 1  (PRIMO)
I= 3\tF= 2  (PRIMO)
I= 4\tF= 3  (PRIMO)
I= 5\tF= 5  (PRIMO)
I= 6\tF= 8 
I= 7\tF= 13  (PRIMO)
I= 8\tF= 21 
I= 9\tF= 34 
I= 10\tF= 55 
I= 11\tF= 89  (PRIMO)
I= 12\tF= 144 
I= 13\tF= 233  (PRIMO)
I= 14\tF= 377 
I= 15\tF= 610 
I= 16\tF= 987 
I= 17\tF= 1597  (PRIMO)
I= 18\tF= 2584 
I= 19\tF= 4181 
I= 20\tF= 6765 
I= 21\tF= 10946 
I= 22\tF= 17711 
I= 23\tF= 28657  (PRIMO)
I= 24\tF= 46368 
I= 25\tF= 75025 
I= 26\tF= 121393 
I= 27\tF= 196418 
I= 28\tF= 317811 
I= 29\tF= 514229  (PRIMO)
I= 30\tF= 832040 '''    
),
(
r'''10 S=285 : R=9 : D=10^R
20 F$="0"+STRING$(R-1,"#")
30 DIM F(S)
40 N=100 'S=285 -> N hasta 999
50 T1=TIME
60 L=0 : F(1)=V=1
70 FOR C=2 TO N
80 FOR T=1 TO V
90 B=F(T)*C+L : L=B\D
100 F(T)=B MOD D
110 NEXT T
120 IF L THEN F(T)=L:V=T:L=0
130 NEXT C
140 T1=TIME-T1
150 'PRINT USING "#";T1;" seg."
160 PRINT STR$(F(T-1));
170 FOR K=T-2 TO 1 STEP -1 : PRINT USING F$; F(K); : NEXT
180 END''',

'''93326215443944152681699238856266700490715968264381621468592963895217599993229915608941463976156518286253697920827223758251185210916864000000000000000000000000'''
),
(
'''10 REM CAPICUA
20 T=TIME
30 FOR A=1 TO 9
40 FOR B=0 TO 9
50 FOR C=0 TO 9
60 N=100001*A+10010*B+1100*C
70 R=SQR(N)
80 IF INT(R+.5)*INT(R+.5)=N THEN GOTO 120
90 NEXT C
100 NEXT B
110 NEXT A
120 T=TIME-T
130 PRINT "EL NÚMERO ES";N
140 'PRINT USING "#.#";T;" SEG."
150 END''',

'''EL NÚMERO ES 698896'''
),
(
'''10 REM CAPICUA 2
20 FOR R=315 TO 1000
30 N=R*R
40 A$=STR$(N)
50 B$=MID$(A$,3,1)+MID$(A$,2,1)+MID$(A$,1,1)
60 IF B$=RIGHT$(A$,3) THEN GOTO 80
70 NEXT R
80 PRINT "EL NÚMERO ES";N
90 END''',

'''EL NÚMERO ES 698896'''
),
(
'''100 REM CALCULO DE PI CLASICO
110 N1 = 100
120 PRINT "NUMERO DE CIFRAS:"; N1
130 LET T1 = TIME
140 LET N2 = INT(N1 * 3.33) + 1
150 LET N3 = INT(N1 / 10) + 2
160 LET N4 = N3 + 1
170 DIM C(N4)
180 LET C(1) = N2 * 2
190 FOR N2 = N2 TO 1 STEP -1
200 LET C2 = INT(N2 * 2) + 1
210 LET B = C(1)
220 FOR T = 1 TO N3
230 LET C(T) = INT(B / C2)
240 LET B = C(T + 1) + (B - C(T) * C2) * 1e10
250 NEXT T
260 LET C(1) = C(1) + 2
270 LET N = N2 - 1
280 IF N <> 0 THEN GOTO 360
290 LET T2 = TIME
300 FOR T = 1 TO N3
310 PRINT USING "0#########"; C(T);
315 IF T MOD 7 = 0 THEN PRINT
320 NEXT T
330 PRINT
340 'PRINT USING "#.#"; "TIEMPO EMPLEADO: "; T2 - T1; " SEG."
350 STOP
360 LET G = 0
370 FOR T = N3 TO 1 STEP -1
380 LET F = N / 1e10 * C(T)
390 LET C(T) = INT(G + (F - INT(F)) * 1e10 + 0.5)
400 LET G = INT(F)
410 NEXT T
420 NEXT N2
430 END''',

'''NUMERO DE CIFRAS: 100
0000000003141592653589793238462643383279502884197169399375105820974944
59230781640628620899862803482534211706797938811007'''
),(
r'''100 REM BASIC SPIGOT PI PORT
110 LET N1 = 100
120 LET L = (N1 \ 4 + 1) * 14
130 DIM A(L)
140 LET E = 0 : LET D = 0
150 LET F = 10000
160 LET H = 0
170 FOR C = L TO 1 STEP -14
180 FOR B = C - 1 TO 1 STEP -1
190 LET D = D * B
200 IF H=0 THEN LET D = D + 2000 * F ELSE LET D = D + A(B) * F
210 LET G = B + B - 1
220 LET A(B) = D MOD G
230 LET D = D \ G
240 NEXT B
250 PRINT USING "0###"; (E + D \ F);
260 LET H = 1
270 LET E = D MOD F
280 LET D = E
290 NEXT C
300 PRINT
310 END''',

'''31415926535897932384626433832795028841971693993751058209749445923078164062862089986280348253421170679821'''
),
(
'''10 REM Simultaneous solution of Hilbert matrices
11 REM by Gaussian elimination, Apr 20, 81
40 A$ ="#.##### "
70 M1 = 11
80 DIM Z(M1), A(M1,M1), C1(M1), W(M1,1), B(M1,M1), I2(M1,3)
90 REM
100 N1 = 2
110 N2 = N1
120 A(1,1) = 1.0
130 FOR J2 = 2 TO M1
140 GOSUB 500 : REM input subroutine
150 GOSUB 5000 : REM Gauss - Jordan subroutine
260 PRINT "        Solution"
270 PRINT
280 FOR I = 1 TO N2
290 PRINT USING A$; C1(I);
300 NEXT I
310 PRINT
320 PRINT
330 N1 = N1 + 1
340 N2 = N1
350 NEXT J2
360 'PRINT CHR$(7)
370 GOTO 9999
500 REM
510 REM input the data
520 REM
530 FOR I = 1 TO N1
540 A(N1,I) = 1.0 / (N1 + I - 1)
550 A(I,N1) = A(N1,I)
560 NEXT I
570 A(N1,N1) = 1.0 / (2 * N1 - 1)
580 FOR I = 1 TO N1
590 Z(I) = 0
600 FOR J = 1 TO N1
610 Z(I) = Z(I) + A(I,J)
620 NEXT J
630 NEXT I
640 RETURN : REM from input routine
5000 REM Gauss - Jordan matrix inversion and solution (Continue with lines 5010 to 6130 of Figure 4.6.)
5010 REM Apr 20, 81
5080 REM end of identifiers
5090 E1 = 0 : REM becomes 1 for singular matrix
5100 I5 = 1 : REM print inverse matrix if zero
5110 N3 = 1 : REM number of constant vectors
5120 FOR I = 1 TO N2
5130 FOR J = 1 TO N2
5140 B(I,J) = A(I,J)
5150 NEXT J
5160 W(I,1) = Z(I)
5170 I2(I,3) = 0
5180 NEXT I
5190 D3 = 1
5200 FOR I = 1 TO N2
5210 REM
5220 REM search for largest (pivot) element
5230 REM
5240 B1=0
5250 FOR J = 1 TO N2
5260 IF (I2(J,3) = 1) THEN 5350
5270 FOR K = 1 TO N2
5280 IF (I2(K,3) > 1) THEN 6120
5290 IF (I2(K,3) = 1) THEN 5340
5300 IF (B1 >= ABS(B(J,K))) THEN 5340
5310 I3 = J
5320 I4 = K
5330 B1 = ABS(B(J,K))
5340 NEXT K
5350 NEXT J
5360 I2(I4,3) = I2(I4,3) + 1
5370 I2(I,1) = I3
5380 I2(I,2) = I4
5390 REM interchange rows to put pivot on diagonal
5400 IF (I3 = I4) THEN 5540
5410 D3 = -D3
5420 FOR L = 1 TO N2
5430 H1 = B(I3,L)
5440 B(I3,L) = B(I4,L)
5450 B(I4,L) = H1
5460 NEXT L
5470 IF (N3 < 1) THEN 5540
5480 FOR L = 1 TO N3
5490 H1 = W(I3,L)
5500 W(I3,L) = W(I4,L)
5510 W(I4,L) = H1
5520 NEXT L
5530 REM divide pivot row by pivot element
5540 P1 = B(I4,I4)
5550 D3 = D3 * P1
5560 B(I4,I4) = 1
5570 FOR L = 1 TO N2
5580 B(I4,L) = B(I4,L) / P1
5590 NEXT L
5600 IF (N3 < 1) THEN 5660
5610 FOR L = 1 TO N3
5620 W(I4,L) = W(I4,L) / P1
5630 NEXT L
5640 REM
5650 REM reduce nonpivot rows
5660 FOR L1 = 1 TO N2
5670 IF (L1 = I4) THEN 5770
5680 T = B(L1,I4)
5690 B(L1,I4) = 0
5700 FOR L = 1 TO N2
5710 B(L1,L) = B(L1,L) - B(I4,L) * T
5720 NEXT L
5730 IF (N3 < 1) THEN 5770
5740 FOR L = 1 TO N3
5750 W(L1,L) = W(L1,L) - W(I4,L) * T
5760 NEXT L
5770 NEXT L1
5780 NEXT I
5790 REM
5800 REM interchange columns
5810 REM
5820 FOR I = 1 TO N2
5830 L = N2 - I + 1
5840 IF (I2(L,1) = I2(L,2)) THEN 5920
5850 I3 = I2(L,1)
5860 I4 = I2 (L,2)
5870 FOR K = 1 TO N2
5880 H1 = B(K,I3)
5890 B(K,I3) = B(K,I4)
5900 B(K,I4) = HI
5910 NEXT K
5920 NEXT I
5930 FOR K = 1 TO N2
5940 IF (I2(K,3) <> 1) THEN 6120
5950 NEXT K
5960 E1 = 0
5970 FOR I = 1 TO N2
5980 C1(I) = W(I,1)
5990 NEXT I
6000 IF (I5 = 1) THEN 6140
6010 PRINT
6020 PRINT " Matrix inverse"
6030 FOR I = 1 TO N2
6040 FOR J = 1 TO N2
6050 PRINT USING A$; B(I,J);
6060 NEXT J
6070 PRINT
6080 NEXT I
6090 PRINT
6100 PRINT "Determinant = "; D3
6110 RETURN : REM if inverse is printed
6120 E1 = 1
6130 PRINT "ERROR - matrix singular "
6140 RETURN : REM from Gauss - Jordan subroutine
9999 END''',

'''        Solution

1.00000 1.00000 

        Solution

1.00000 1.00000 1.00000 

        Solution

1.00000 1.00000 1.00000 1.00000 

        Solution

1.00000 1.00000 1.00000 1.00000 1.00000 

        Solution

1.00000 1.00000 1.00000 1.00000 1.00000 1.00000 

        Solution

1.00000 1.00000 1.00000 1.00000 1.00000 1.00000 1.00000 

        Solution

1.00000 1.00000 1.00000 1.00000 1.00000 1.00000 1.00000 1.00000 

        Solution

1.00000 1.00000 1.00000 1.00000 0.99999 1.00001 0.99998 1.00001 1.00000 

        Solution

1.00000 1.00000 1.00000 1.00002 0.99990 1.00028 0.99955 1.00044 0.99977 1.00005 

        Solution

1.00000 1.00000 0.99998 1.00026 0.99843 1.00556 0.98785 1.01661 0.98616 1.00642 0.99873 
'''
),
(
'''10 REM DISTRIBUCION BIDIMENSIONAL
20 N=12 : PRINT "NUMERO DE PARES DE VALORES:";N
30 DIM D(2,N), P(2,2)
40 FOR F=1 TO 2
50 FOR C=1 TO N: READ D(F,C): NEXT C
60 NEXT F
70 FOR F=1 TO 2 : M=0 : V=0
80 FOR C=1 TO N: M=M+D(F,C) : NEXT C
90 P(F,1)=M/N
100 FOR C=1 TO N : V=V+(ABS(D(F,C)-P(F,1)))^2 : NEXT C
110 P(F,2)=V/N
120 NEXT F
130 PRINT
140 FOR F=1 TO 2
150 ON F GOTO 160, 170
160 PRINT "PRIMERA VARIABLE" : GOTO 180
170 PRINT "SEGUNDA VARIABLE"
180 PRINT : PRINT "MEDIA=";P(F,1),"VARIANZA=";P(F,2)
190 PRINT "DESV. TIPICA=";SQR(P(F,2)) : PRINT
200 NEXT F
210 C0=0
220 FOR C=1 TO N : C0=C0+(D(1,C)-P(1,1))*(D(2,C)-P(2,1)) : NEXT C
230 C0=C0/N : PRINT : PRINT "COVARIANZA=";C0
240 PRINT "COEF. DE CORRELACION=";C0/SQR(P(1,2)* P(2,2))
250 PRINT : PRINT "RECTAS DE REGRESION": PRINT
260 PRINT "Y-(";P(2,1);")=";C0/P(1,2);"(X-(";P(1,1);"))"
270 PRINT "Y-(";P(2,1);")=";P(2,2)/C0;"(X-(";P(1,1);"))"
280 DATA 6,5,6,4,5,2,5,3,1,3,4,2,3,6,2,6,3,4,4,5,2,2,1,4,3,1,6,2,4,4
290 DATA 4,3,2,2,3,1,2,3,2,2,4,3,4,2,1,4,3,3,2,3,1,2,3,2,2,1,4,3,3,2
300 END''',

'''NUMERO DE PARES DE VALORES: 12

PRIMERA VARIABLE

MEDIA= 3.8333333333333\tVARIANZA= 2.4722222222222
DESV. TIPICA= 1.5723301886761

SEGUNDA VARIABLE

MEDIA= 3.5\tVARIANZA= 2.4166666666667
DESV. TIPICA= 1.5545631755148


COVARIANZA= 0.083333333333333
COEF. DE CORRELACION= 0.034093110421689

RECTAS DE REGRESION

Y-( 3.5 )= 0.033707865168539 (X-( 3.8333333333333 ))
Y-( 3.5 )= 29 (X-( 3.8333333333333 ))'''
),
(
'''10 REM VIAJE RELATIVISTA
20 C= 300000
30 D=10 : PRINT "DISTANCIA A QUE HEMOS DE VIAJAR (EN AÑOS LUZ):"; D
40 V1 = 100000 : V2 = 250000 : PRINT "VALORES EXTREMOS DE LA VELOCIDAD:";V1;",";V2
50 I0= (V2-V1)/10
100 PRINT
110 PRINT "DISTANCIA";D;"AÑOS LUZ" : PRINT
120 PRINT "VELOCIDAD","DIAS/AÑO","T.VIAJE"
130 FOR N= 0 TO 31 : PRINT ""; : NEXT N
140 PRINT
200 FOR V=V1 TO V2 STEP I0
210 T=365*SQR(1-(V/C)^2)
220 R$= MID$(STR$(T-INT(T)),2,4)
230 T=INT(T)+VAL(R$)
240 T1=D*SQR(1-(V/C)^2)
250 R$=MID$(STR$(T1-INT(T1)),2,4)
260 T1= INT(T1)+VAL(R$)
270 PRINT V,,DEC$(T,"#.###"),,T1
280 NEXT V
290 END''',

'''DISTANCIA A QUE HEMOS DE VIAJAR (EN AÑOS LUZ): 10
VALORES EXTREMOS DE LA VELOCIDAD: 100000 , 250000

DISTANCIA 10 AÑOS LUZ

VELOCIDAD\tDIAS/AÑO\tT.VIAJE

 100000\t\t344.125\t\t 9.428
 115000\t\t337.117\t\t 9.236
 130000\t\t328.950\t\t 9.012
 145000\t\t319.534\t\t 8.754
 160000\t\t308.755\t\t 8.459
 175000\t\t296.464\t\t 8.122
 190000\t\t282.465\t\t 7.738
 205000\t\t266.488\t\t 7.301
 220000\t\t248.152\t\t 6.798
 235000\t\t226.884\t\t 6.216
 250000\t\t201.761\t\t 5.527'''
),
(
'''10 REM EVOLUCION DE LA POBLACION
20 PRINT
30 PRINT "REPRESENTACION DE LA EVOLUCION"
40 PRINT "DE LA POBLACION DE UNA CIUDAD"
50 PRINT "MEDIANTE DIAGRAMA DE BARRAS"
60 PRINT : PRINT
70 PRINT "PARA VER EL DIAGRAMA PULSE"
80 PRINT "ENTER"
90 REM INPUT "",Y$
120 M=0
130 FOR I=1 TO 10
140 READ A(I) ,P(I)
150 IF P(I)>M THEN M=P(I)
160 NEXT I
170 REM REPRESENTACION DIAGRAMA
180 FOR I=1 TO 10
190 PRINT : PRINT : PRINT A(I);
200 C(I) =INT(P(I) *34/M+0.5)
210 FOR J=1 TO C(I)
220 PRINT CHR$(35);
230 NEXT J
240 NEXT I
245 PRINT
250 DATA 1935,1238400,1940,1401107
260 DATA 1945, 1580880,1950,1786055
270 DATA 1955, 2053963,1960, 2362057
280 DATA 1965, 2716366,1970,3123821
290 DATA 1975, 3592394, 1980,4237525
300 END''',

'''
REPRESENTACION DE LA EVOLUCION
DE LA POBLACION DE UNA CIUDAD
MEDIANTE DIAGRAMA DE BARRAS


PARA VER EL DIAGRAMA PULSE
ENTER


 1935 ##########

 1940 ###########

 1945 #############

 1950 ##############

 1955 ################

 1960 ###################

 1965 ######################

 1970 #########################

 1975 #############################

 1980 ##################################'''
),
(
'''100 REM BANDERA U.S.A.
110 PRINT
120 FOR F=1 TO 3
130 GOSUB 700: REM ESTRELLAS Y BARRAS
140 GOSUB 500: REM ESTRELLAS
150 PRINT
160 NEXT F
170 GOSUB 700: REM ESTRELLAS Y BARRAS
180 FOR F=5 TO 7
190 PRINT
200 GOSUB 800: REM BARRAS Y BARRAS
210 NEXT F
220 END
500 REM SUBRUTINA ESTRELLAS
510 FOR E=1 TO 7
520 PRINT "*";
530 NEXT E
540 RETURN
600 REM SUBRUTINA BARRA
610 FOR C=1 TO 7
620 PRINT "#";
630 NEXT C
640 RETURN
700 REM SUBRUTINA ESTRELLAS Y BARRAS
710 GOSUB 500 : REM ESTRELLAS
720 GOSUB 600 : REM BARRA
730 GOSUB 600 : REM BARRA
740 PRINT
750 RETURN
800 REM SUBRUTINA BARRAS Y BARRAS
810 GOSUB 600 : REM BARRA
820 GOSUB 600 : REM BARRA
830 GOSUB 600 : REM BARRA
840 PRINT
850 RETURN''',

'''
*******##############
*******
*******##############
*******
*******##############
*******
*******##############

#####################

#####################

#####################'''
),
(
r'''100 REM ALGORITMO DE MACHIN PARA CALCULAR PI
110 P=100 : PRINT "DIGITOS:";P
120 L=11 : F=10^L : G=(P+1)\L
130 DIM B1(G),B2(G),B3(G),B4(G)
140 T1=TIME
150 P=5 : GOSUB 900
160 P=16 : GOSUB 610
170 P=239 : GOSUB 900
180 P=4 : GOSUB 600
190 GOSUB 500
200 T2=TIME
210 REM IMPRIME
220 FOR A=0 TO G
230 PRINT USING "0########## ";B1(A);
240 IF A>0 AND A MOD 6 = 5 THEN PRINT
250 NEXT A
260 'PRINT : PRINT USING "#.#";T2-T1;" SEG."
270 STOP
300 REM B4=0?
310 Z=1
320 FOR A=G TO 0 STEP -1
330 IF B4(A) THEN Z=0:GOTO 350
340 NEXT A
350 RETURN
400 REM B1+B2->B1
410 K=0 : D=0
420 FOR A=G TO 0 STEP -1
430 D=B1(A)+B2(A)+K
440 IF D>F THEN D=D-F:K=1 ELSE K=0
450 B1(A)=D
460 NEXT A
470 RETURN
500 REM B1-B2->B1 O B3-B2->B1
505 T=1 : GOTO 520
510 T=0
520 K=0
530 FOR A=G TO 0 STEP -1
540 IF T THEN D=B3(A)-B2(A)-K ELSE D=B1(A)-B2(A)-K
550 IF D<0 THEN D=D+F:K=1 ELSE K=0
560 B1(A)=D
570 NEXT A
580 RETURN
600 REM B1*F1->B2 O B1*F1->B3, B1=0
605 T=0 : GOTO 620
610 T=1
620 K=0
630 FOR A=G TO 0 STEP -1
640 IF B1(A)=K THEN 680
650 D=B1(A)*P+K
660 IF D>F THEN K=D\F:D=D MOD F ELSE K=0
670 IF T THEN B3(A)=D:B1(A)=0 ELSE B2(A)=D
680 NEXT A
690 RETURN
700 REM B4/F1->B2 O B4/F1->B4
705 T=0 : GOTO 720
710 T=1
720 R=0
730 FOR A=0 TO G
740 IF B4(A)=R THEN B2(A)=0:GOTO 780
750 D=R*F+B4(A)
760 C=D\P : R=D MOD P
770 IF T THEN B4(A)=C ELSE B2(A)=C
780 NEXT A
790 RETURN
800 REM 1/F->B4
810 R=F : D=0
820 FOR A=0 TO G
830 B4(A)=R\P
840 R=(R MOD P)*F
850 NEXT A
860 RETURN
900 REM ATAN(1/F1)->B1
910 S=1 : N=1 : Q=P*P
920 GOSUB 800
930 P=N : GOSUB 700
940 ON S GOSUB 400,510
950 P=Q : GOSUB 710
960 N=N+2 : S=3-S
970 GOSUB 300 : IF Z=0 THEN 930
980 RETURN
990 END''',

'''DIGITOS: 100
14159265358 97932384626 43383279502 88419716939 93751058209 74944592307 
81640628620 89986280348 25342117067 98214808656 '''
),
(
'''100 REM N-Queens
110 N=8
120 DIM Colmn(N)
130 Prev=Rw=0
140 Colmn(Rw)=-1
150 Done1=Done2=0
160 WHILE Rw>=0 AND NOT Done1
170     Colmn(Rw)=Colmn(Rw)+1
180     WHILE Colmn(Rw)<N AND NOT Done2
190             Valid=1
200             FOR Prev=0 TO Rw-1
210                     IF (Colmn(Prev)=Colmn(Rw) OR ABS(Colmn(Prev)-Colmn(Rw))=(Rw-Prev)) THEN Valid=0:GOTO 230
220             NEXT
230             IF Valid THEN Done2=-1 ELSE Colmn(Rw)=Colmn(Rw)+1
240     WEND
250     Done2=0
260     IF Colmn(Rw)>=N THEN Colmn(Rw)=0:Rw=Rw-1:GOTO 280
270     IF Rw=N-1 THEN Done1=-1 ELSE Rw=Rw+1:Colmn(Rw)=-1
280 WEND
290 IF NOT Done1 THEN PRINT "No hay solución.":END
300 PRINT " "; : FOR count=1 TO N : PRINT " "+STR$(count); : NEXT : PRINT
310 PRINT STRING$(2*N+1,"-")
320 FOR prow=0 TO N-1
330     PRINT STR$(prow+1)+"|";
340     FOR C=0 TO N-1
350             IF Colmn(prow)=C THEN PRINT "X "; ELSE PRINT "- ";
360     NEXT
370     PRINT
380 NEXT''',

'''  1 2 3 4 5 6 7 8
-----------------
1|X - - - - - - - 
2|- - - - X - - - 
3|- - - - - - - X 
4|- - - - - X - - 
5|- - X - - - - - 
6|- - - - - - X - 
7|- X - - - - - - 
8|- - - X - - - - '''
),
(
'''5 REM AJUSTE DE CURVAS POR MÍNIMOS CUADRADOS
6 MAT BASE 1
10 DIM X(100),Y(100)
15 PRINT "INTRODUZCA N=0 PARA FUNCIÓN POTENCIA, N=1 PARA FUNCIÓN ";
20 PRINT "EXPONENCIAL."
25 PRINT "PARA POLINOMIAL, HAGA N IGUAL AL NÚMERO DE TÉRMINOS EN ";
30 PRINT "LA POLINOMIAL."
35 PRINT "N=";
40 N=5 'INPUT N
45 '
50 REM             INTRODUCIR PUNTOS
55 '
56 PRINT
57 PRINT "INTRODUZCA EL NÚMERO DE PUNTOS."
59 M=19 'INPUT M
60 REDIM X(M),Y(M)
61 PRINT
65 PRINT "INTRODUZCA LOS VALORES DE Y"
70 MAT READ Y
80 PRINT
85 PRINT "INTRODUZCA LOS VALORES DE X"
90 MAT READ X
115 '
120 REM             CALCULAR LOS LOGARITMOS DE X E Y SI ES NECESARIO
125 '
130 IF N>=2 THEN 170
135 FOR I=1 TO M
140    LET Y(I)=LOG(Y(I))
145 NEXT I
150 IF N=1 THEN 170
155 FOR I=1 TO M
160    LET X(I)=LOG(X(I))
165 NEXT I
170 '
175 REM             CALCULAR LOS ELEMENTOS DE LA MATRIZ A Y EL VECTOR D
180 '
185 LET N1=N
190 IF N1>=2 THEN 200
195 LET N1=2
200 DIM A(N1,N1),D(N1)
201 MAT A=ZER
205 MAT D=ZER
210 FOR I=1 TO N1
215    FOR J=1 TO N1
220       IF I+J>2 THEN 235
225       LET A(I,J)=M
230       GOTO 250
235       FOR K=1 TO M
240          LET A(I,J)=A(I,J)+X(K)^(I+J-2)
245       NEXT K
250    NEXT J
255    FOR K=1 TO M
260       IF I>1 THEN 275
265       LET D(I)=D(I)+Y(K)
270       GOTO 280
275       LET D(I)=D(I)+Y(K)*X(K)^(I-1)
280    NEXT K
285 NEXT I
290 '
295 REM             IMPRIMIR LAS ECUACIONES LINEALES SIMULTANEAS
300 '
305 PRINT
310 PRINT "COEFICIENTES DEL SISTEMA DE ECUACIONES LINEALES"
315 PRINT
320 MAT PRINT A,
321 PRINT
325 MAT PRINT D
355 '
360 REM             RESOLVER LAS ECUACIONES LINEALES SIMULTANEAS
365 '
370 MAT B=INV(A)
375 MAT C=B*D
380 '
385 REM             IMPRIMIR LA ECUACIÓN AJUSTADA A LA CURVA
390 '
395 IF N>1 THEN 430
400 LET C1=EXP(C(1))
405 IF N=1 THEN 420
410 PRINT "FUNCIÓN POTENCIA: Y=";C1;"* X ^";C(2)
415 GOTO 490
420 PRINT "FUNCIÓN EXPONENCIAL: Y=";C1;"* EXP(";C(2);"* X )"
425 GOTO 490
430 IF C(2)>0 THEN 445
435 PRINT "FUNCIÓN POLINOMIAL: Y=";C(1);"-";ABS(C(2));"* X ";
440 GOTO 450
445 PRINT "FUNCIÓN POLINOMIAL: Y=";C(1);"+";C(2);"* X ";
450 IF N=2 THEN 485
455 FOR I=3 TO N
460    IF C(I)>0 THEN 475
465    PRINT "-";ABS(C(I));"* X ^";I-1;
470    GOTO 480
475    PRINT "+";C(I);"* X ^";I-1;
480 NEXT I
485 PRINT
490 '
495 REM     IMPRIMIR VALORES DE 'Y' INTRODUCIDOS Y CALCULADOS
500 '
505 IF N>=2 THEN 545
510 FOR I=1 TO M
515    LET Y(I)=EXP(Y(I))
520 NEXT I
525 IF N=1 THEN 545
530 FOR I=1 TO M
535    LET X(I)=EXP(X(I))
540 NEXT I
545 PRINT
550 PRINT "X","Y REAL","Y CALCULADO"
555 LET S=0
560 FOR I=1 TO M
565    IF N>=2 THEN 595
570    IF N=1 THEN 585
575    LET Y1=C1*X(I)^C(2)
580    GOTO 615
585    LET Y1=C1*EXP(C(2)*X(I))
590    GOTO 615
595    LET Y1=C(1)
600    FOR J=2 TO N
605       LET Y1=Y1+C(J)*X(I)^(J-1)
610    NEXT J
615    LET S=S+(Y(I)-Y1)^2
620    PRINT X(I),Y(I),Y1
625 NEXT I
630 PRINT
635 PRINT "SUMA DE LOS CUADRADOS DE LOS ERRORES=";S
640 END
900 DATA 0.01, 0.02, 0.02, 0.03, 0.03, 0.04, 0.04, 0.09, 0.24, 0.38, 0.63, 0.93, 1.24, 1.48, 1.73, 2.07, 2.5, 3.12, 3.48
905 DATA 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25''',

'''INTRODUZCA N=0 PARA FUNCIÓN POTENCIA, N=1 PARA FUNCIÓN EXPONENCIAL.
PARA POLINOMIAL, HAGA N IGUAL AL NÚMERO DE TÉRMINOS EN LA POLINOMIAL.
N=
INTRODUZCA EL NÚMERO DE PUNTOS.

INTRODUZCA LOS VALORES DE Y

INTRODUZCA LOS VALORES DE X

COEFICIENTES DEL SISTEMA DE ECUACIONES LINEALES

                    19                   304                  5434                105184               2151370
                   304                  5434                105184               2151370              45723424
                  5434                105184               2151370              45723424             998814154
                105184               2151370              45723424             998814154           22267168864
               2151370              45723424             998814154           22267168864          504211511050

 18.08
 394.84
 8773.92
 197719
 4507584
FUNCIÓN POLINOMIAL: Y=-0.73370762434206 + 0.33278413432345 * X - 0.048084033912687 * X ^ 2 + 0.0025728002874672 * X ^ 3 - 3.6476625188242E-05 * X ^ 4 

X\tY REAL\tY CALCULADO
 7\t 0.01\t 0.03455377572468
 8\t 0.02\t 0.019052770245708
 9\t 0.02\t 0.0027911093448397
 10\t 0.03\t-0.0062356367915211
 11\t 0.03\t-0.00090733698149081
 12\t 0.04\t 0.025020700952297
 13\t 0.04\t 0.07691773018269
 14\t 0.09\t 0.15927756487802
 15\t 0.24\t 0.2757185802021
 16\t 0.38\t 0.42898371231422
 17\t 0.63\t 0.62094045836916
 18\t 0.93\t 0.85258087651719
 19\t 1.24\t 1.124021585904
 20\t 1.48\t 1.4345037666709
 21\t 1.73\t 1.7823931599546
 22\t 2.07\t 2.1651800678872
 23\t 2.5\t 2.5794793535964
 24\t 3.12\t 3.0210304412053
 25\t 3.48\t 3.4846973158327

SUMA DE LOS CUADRADOS DE LOS ERRORES= 0.062765106441994'''
),
(
'''10 DIM V(4)
20 FOR I=0 TO 4: V(I)=I+1: NEXT
30 DIM M(1,1)
40 FOR I=0 TO 1: FOR J=0 TO 1: M(I,J)=I*10+J+1: NEXT J: NEXT I
50 MAT PRINT COL V; ROW V;
60 MAT PRINT ROW M, COL M;
70 MAT PRINT USING "##"; COL V;
80 MAT BASE 1
90 MAT PRINT COL V
100 MAT BASE 0
110 MAT PRINT COL V;
120 END''',

''' 1   2   3   4   5

 1
 2
 3
 4
 5
                     1                     2
                    11                    12

 1   11
 2   12
 1   2   3   4   5
 2   3   4   5
 1   2   3   4   5'''
),
(
'''10 DIM A(2),B(2)
20 MAT A=2 : MAT B=3
30 DEF FNSUM(X,Y)=X(0)+Y(1)
40 PRINT FNSUM(A,B)''',

'''Line 40. Invalid value type.'''
),
(
'''10 DEF FNMUL(X,Y)
20 FNMUL = X(0)*Y(0)
30 FNEND
40 DIM A(1),B(1)
50 MAT A=2 : MAT B=4
60 PRINT FNMUL(A,B)''',

''' 8'''
),
(
'''10 DEF FNM(x,y)
20 MAT FNM=x+y
40 FNEND
50 DIM a(1,1),b(1,1)
60 MAT a=3 : MAT b=2
70 MAT c=FNM(a,b)
80 MAT PRINT c''',

''' 5   5
 5   5'''   
),
(
'''10 MAT BASE 1
20 DATA 11,22,33,101,102,103,201,202,203',
30 DIM r(3), m(2,3)',
40 MAT READ r, m',
50 MAT PRINT COL r',
60 MAT PRINT ROW m',
70 END''',

''' 11   22   33
 101   102   103
 201   202   203''',
),
(
'''10 DATA 1,2
20 DIM r(3)
30 MAT READ r
40 END''',

'''Line 30. No more DATA to read.'''
),
(
'''10 DATA "hola"
20 DIM r(0)
30 MAT READ r
40 END''',

'''Line 30. Invalid value type.'''
),
(
'''10 DATA 42
20 DIM labels$(0)
30 MAT READ labels$
40 END''',

'''Line 30. Invalid value type.'''
),
(
'''10 DIM a(1,1,1)
20 MAT READ a
30 END''',

'''Line 20. Invalid number of dimensions.'''
),
(
'''10 MAT BASE 1
15 DIM n(2,5)
25 DATA 1920, 1930, 1940, 1950, 1960
30 DATA 14, 38, 48, 62, 87
35 DATA "Millions of", "U.S. Drivers"
40 MAT READ n
45 READ a$,b$
50 PRINT a$
55 PRINT b$
60 MAT PRINT COL n
65 END''',

'''Millions of
U.S. Drivers
 1920   14
 1930   38
 1940   48
 1950   62
 1960   87'''
),
(
'''10 MAT BASE 1
20 DIM a(3,3)
30 DATA 1, 2, 3, 4, 5, 6, 7, 8, 9
40 MAT READ a
50 MAT PRINT a
60 REDIM a(4,2)
70 MAT PRINT a
80 REDIM a(3,3)
90 MAT PRINT a''',

''' 1   2   3
 4   5   6
 7   8   9
 1   2
 4   5
 7   8
 0   0
 1   2   0
 4   5   0
 7   8   0'''
),
(
'''10 DEF FNM(X,Y)
20 MAT FNM=X+Y
40 FNEND
50 DIM A(1,1),B(1,1),C(1,1)
60 MAT A=CON
70 MAT B=2*A
80 MAT C=FNM(A,B)
90 MAT PRINT C''',

''' 3   3
 3   3'''
),
(
'''10 DIM A(1,1),B(1,1)
20 MAT A=CON
30 DEF FNR(X)
40 MAT FNR=X
50 FNEND
60 MAT B=FNR(A)
70 MAT PRINT B''',

''' 1   1
 1   1'''
),
(
'''10 DIM A(2),B(1)
20 A(0)=5
30 A(2)=9
40 B(1)=7
50 REDIM A(3),B(2)
60 PRINT A(0);A(2);A(3)
70 PRINT B(0);B(1);B(2)
80 END''',

''' 5  9  0
 0  7  0'''
),
(
'''10 DIM C(1,1)
20 FOR I=0 TO 1
30 FOR J=0 TO 1
40 C(I,J)=I*10+J
50 NEXT J
60 NEXT I
70 MAT PRINT USING "##";C
80 PRINT
90 X=3
100 Y=4
110 REDIM C(X-1,Y/2)
120 PRINT C(0,0),C(1,1),C(2,2)
130 PRINT C(0,1),C(2,1),C(2,2)
140 PRINT
150 MAT PRINT USING "##";C
160 END''',

''' 0   1
10  11

 0\t 11\t 0
 1\t 0\t 0

 0   1   0
10  11   0
 0   0   0'''
),
(
'''10 MAT BASE 0
20 DIM N$(2,1)
30 DATA "a","b","c","d","e","f"
40 MAT READ N$
50 MAT PRINT N$
60 REDIM N$(1,0)
70 MAT PRINT N$
80 REDIM N$(2,1)
90 MAT PRINT N$
100 END''',

'a  b\nc  d\ne  f\na\nc\na  \nc  \n  '
),
(
'''10 REDIM A(2)
20 END''',

'''Line 10. Undefined variable or function.'''
),
(
'''10 DIM A(2,2)
20 REDIM A(5)
30 END''',

'''Line 20. Invalid number of dimensions.'''
),
(
'''10 DIM A(1)
20 REDIM A(-2)
30 END''',

'''Line 20. Invalid value.'''
),
(
'''10 DIM A(3,3)
20 REDIM A(1,1)
30 PRINT A(1,2)
40 END''',

'''Line 30. Index out of range.'''
),
(
'''10 DIM A(2)
20 MAT BASE 1
30 MAT A = CON
40 MAT BASE 0
50 MAT PRINT ROW A;
60 MAT A = ZER
70 MAT PRINT ROW A;
80 X=5
90 MAT A = (X+1)
100 MAT PRINT ROW A;
110 END''',

''' 1
 1
 1
 0
 0
 0
 6
 6
 6'''
),
(
'''10 DIM T$(2)
20 MAT T$ = ("texto")
30 MAT PRINT ROW T$;
40 END''',

'''texto
texto
texto'''
),
(
'''10 DIM T$(2)
20 MAT T$ = 5''',

'''Line 20. Invalid value type.'''
),
(
'''10 DIM A(2,2), C(2,2)
20 MAT A = (2)
30 MAT C = (3*A)
40 MAT A = (A+C)
50 MAT A = (A/2)
60 MAT PRINT A
70 END''',

''' 4   4   4
 4   4   4
 4   4   4'''
),
('''10 DIM A(1,2)
20 FOR I=0 TO 1
30 FOR J=0 TO 2
40 A(I,J)=I*10+J
50 NEXT J
60 NEXT I
70 MAT B = TRN(A)
80 MAT PRINT A
90 PRINT
100 MAT PRINT B
110 END''',

''' 0   1   2
 10   11   12

 0   10
 1   11
 2   12'''
),
('''10 DIM A(1,1)
20 MAT A = IDN
30 T = TRN(A)
40 END''',

'''Line 30. Expression not allowed.'''
),
('''10 DIM A(1,1)
20 MAT A = IDN
30 X = INV(A)
40 END''',

'''Line 30. Expression not allowed.'''
),
('''10 DIM A(1,1),C(1,1)
20 A(0,0)=4
30 A(0,1)=7
40 A(1,0)=2
50 A(1,1)=6
60 MAT B = INV(A)
70 MAT C = A*B
80 MAT PRINT USING "0.###";C
90 PRINT
100 PRINT DET(A)
110 END''',

'''1.000  0.000
0.000  1.000

 10'''
),
('''10 MAT BASE 1
20 DIM A(3,3)
30 DATA 2,1,-1,1,-1,1,1,2,1
40 MAT READ A
50 MAT I = INV(A)
60 MAT PRINT USING "0.###";I
70 PRINT
80 PRINT DET(A)
90 END''',

'''0.333  0.333  0.000
0.000  -0.333  0.333
-0.333  0.333  0.333

-9'''
),
(
'''10 DIM A(0,0)
20 DATA 2
30 MAT READ A
40 MAT A = (A^2)
50 PRINT A(0,0)
60 MAT A = (2^A)
70 PRINT A(0,0)
80 END''',

''' 4
 16'''
),
(
'''10 DIM B(1,1), C(1,1)
20 MAT B = (2)
30 MAT C = (3)
40 MAT D = (B+C)
50 MAT PRINT D
60 END''',

''' 5   5
 5   5'''
),
(
'''10 DIM A(0,0), B(1,2), C(1,2)
20 MAT B = (1)
30 MAT C = (2)
40 MAT A = (B+C)
50 MAT PRINT A
60 END''',

''' 3   3   3
 3   3   3'''
),
(
'''10 DIM A(1,0), B(0,1)
20 DATA 1,2,3,4
30 MAT READ A, B
40 MAT A = (A*B)
50 MAT PRINT A
60 END''',

''' 3   4
 6   8'''
),
(
'''10 DIM B(1,1)
20 MAT B = (2*B/3)
''',

'''Line 20. Expression not allowed.'''
),
(
'''10 DIM A(1,1), B(2,2), C(1,1)
20 MAT C = (A+B)
''',

'''Line 20. Invalid number of dimensions.'''
),
(
'''10 DIM A(1,2), B(2,1), C(1,1)
20 DATA 1,2,3,4,5,6,7,8,9,10,11,12
30 MAT READ A, B
40 MAT C = (A*B)
50 MAT PRINT C
60 END''',

''' 58   64
 139   154'''
),
(
'''10 DIM A(1,0), B(1,1), C(1,1)
20 MAT C = (A*B)
''',

'''Line 20. Invalid number of dimensions.'''
),
(
'''10 DIM A(1,1), B(1,1), C(1,1)
20 MAT C = (A/B)
''',

'''Line 20. Undefined variable or function.'''
),
(
'''10 DIM A(0,0), B(0,0)
20 MAT A = (2)
30 MAT B = (3)
40 MAT C = (A^B)
''',

'''Line 40. Expression not allowed.'''
),
(
'''10 DIM A(0,0), B(0,0)
20 MAT A = (2)
30 MAT B = (3)
40 MAT A = SIN(B)
''',

'''Line 40. Expression not allowed.'''
),
(
'''10 DIM M(3,3)
20 MAT BASE 1
30 MAT M = IDN
40 MAT BASE 0
50 FOR I=0 TO 3
60 FOR J=0 TO 3
70 PRINT M(I,J);
80 NEXT J
90 PRINT
100 NEXT I
110 END''',

''' 0  0  0  0 
 0  1  0  0 
 0  0  1  0 
 0  0  0  1 '''
),
(
'''10 DIM M(1,2)
20 MAT M = IDN
30 END''',

'''Line 20. MAT IDN requires a two-dimensional square matrix.'''
),
(
'''10 MAT BASE 1
15 DIM A(20,20),B(20,20),C(20,20)
20 READ M,N
25 REDIM A(M,N),B(N,N)
30 MAT READ A,B
35 MAT C=A+A
40 MAT PRINT C
45 MAT C=A*B
50 PRINT
55 PRINT "A*B ="
60 MAT PRINT C
65 DATA 2, 3
70 DATA 1, 2, 3
75 DATA 4, 5, 6
80 DATA 1, 0, -1
85 DATA 0, -1, -1
90 DATA -1, 0, 0
95 END''',

''' 2   4   6
 8   10   12

A*B =
-2  -2  -3
-2  -5  -9'''    
),
(
'''10 MAT BASE 1
20 DIM A(3,1), B(3,3)
30 DATA 0, 6, 3
40 DATA 2, 1, -1, 1, -1, 1, 1, 2, 1
50 MAT READ A,B
55 MAT PRINT A;B
60 MAT I=INV(B)
70 MAT c=I*A
80 MAT PRINT c
85 MAT tr=TRN(c)
90 MAT PRINT tr
110 END''',

''' 0
 6
 3

 2   1  -1
 1  -1   1
 1   2   1
 2
-1
 3
 2  -1   3'''
),
(
'''15 MAT BASE 1
20 DIM a(20,20),b(20,20)
25 READ n
30 REDIM a(n,n) : MAT a=CON
35 FOR i=1 TO n
40 FOR j=1 TO n
45 a(i,j)=1/(i+j-1)
50 NEXT j
55 NEXT i
60 MAT b=INV(a)
65 PRINT "INV(A)="
70 MAT PRINT USING "######.###";b
75 PRINT : PRINT "DETERMINANTE DE A=";DET(a)
80 DATA 4
85 END''',

'''INV(A)=
    16.000    -120.000     240.000    -140.000
  -120.000    1200.000   -2700.000    1680.000
   240.000   -2700.000    6480.000   -4200.000
  -140.000    1680.000   -4200.000    2800.000

DETERMINANTE DE A= 1.6534391534393E-07'''    
),
(
'''15 MAT BASE 1
20 DIM A(4,4),B(4,4)
50 MAT A=CON
60 FOR I=1 TO 4
70 FOR J=1 TO 4
75 LET A(I,J)=1/(I+J-1)
80 NEXT J
90 NEXT I
100 MAT B=INV(A)
120 MAT PRINT USING "0####.###";B
199 END''',

'''00016.000  -0120.000  00240.000  -0140.000
-0120.000  01200.000  -2700.000  01680.000
00240.000  -2700.000  06480.000  -4200.000
-0140.000  01680.000  -4200.000  02800.000'''
),
(
'''15 MAT BASE 1
20 DIM A(4,4),B(4,4)
50 MAT A=CON
60 FOR I=1 TO 4
70 FOR J=1 TO 4
75 LET A(I,J)=1/(I+J-1)
80 NEXT J
90 NEXT I
100 MAT B=INV(A)
120 MAT PRINT USING "+0###.###";B
199 END''',

'''+0016.000  -0120.000  +0240.000  -0140.000
-0120.000  +1200.000  -2700.000  +1680.000
+0240.000  -2700.000  +6480.000  -4200.000
-0140.000  +1680.000  -4200.000  +2800.000'''
),
(
'''10 MAT BASE 1
15 DIM a(20,20),b(20,20),c(20,20)
20 READ m,n
25 REDIM a(m,n), b(n,n)
30 MAT READ a,b
35 MAT c=a+a
40 MAT PRINT c;
45 MAT c=a*b
50 PRINT
55 MAT PRINT c
60 DATA 2, 3
65 DATA 1, 2, 3
70 DATA 4, 5, 6
75 DATA 1, 0, -1
80 DATA 0, -1, -1
85 DATA -1, 0, 0
90 END''',

''' 2   4   6
 8   10   12

-2  -2  -3
-2  -5  -9'''    
),
(
'''10 MAT BASE 1
20 DIM A(3,3),X(3,2),B(3,2)
30 DATA 273, 35, 1
40 DATA 150, 8, 1
50 DATA 124, 19, 1
60 DATA 5835, 7362.5
70 DATA 3240, 4085
80 DATA 2775, 3517.5
90 MAT READ A,B
100 MAT I=INV(A) : MAT X=I*B
110 PRINT "PRINTER A"+"   "+"PRINTER B"
120 MAT PRINT USING " ###.##   ";X''',

'''PRINTER A   PRINTER B
  20.00       25.00   
   5.00        7.50   
 200.00      275.00   '''
),
(
'''10 MAT BASE 1
20 DIM A(2,2),B(2,2),C(2,2)
30 DATA 2, 3, 4, 5
40 F$="###.#"
50 MAT READ A
60 MAT PRINT USING F$;A : PRINT
70 MAT B=INV(A)
80 MAT PRINT USING F$;B : PRINT
90 MAT C=B*A
100 MAT PRINT USING F$;C
110 END''',

'''  2.0    3.0
  4.0    5.0

 -2.5    1.5
  2.0   -1.0

  1.0    0.0
  0.0    1.0'''
),
(
'''10 MAT BASE 1
20 READ N
30 DIM L$(N)
40 MAT READ L$
50 FOR I=1 TO N
60 FOR J=1 TO N-1
70 IF L$(J)<L$(J+1) THEN 110
80 LET A$=L$(J)
90 LET L$(J)=L$(J+1)
100 LET L$(J+1)=A$
110 NEXT J
120 NEXT I
130 MAT PRINT COL L$
900 DATA 5, ONE, TWO, THREE, FOUR, FIVE
999 END''',

'''FIVE  FOUR  ONE  THREE  TWO'''
),
(
'''10 MAT BASE 1
20 READ N
30 DIM L$(N)
40 MAT READ L$
50 FOR I=1 TO N
60 FOR J=1 TO N-1
70 IF L$(J)>=L$(J+1) THEN SWAP L$(J),L$(J+1)
110 NEXT J
120 NEXT I
130 MAT PRINT COL L$
900 DATA 5, ONE, TWO, THREE, FOUR, FIVE
999 END''',

'''FIVE  FOUR  ONE  THREE  TWO'''
),
(
'''10 ON ERROR GOTO 60
15 DIM F$(4,4)
20 MAT F$=4 'TYPE_MISMATCH = (5, "Invalid value type.")
25 DIM B$(4,4)
30 MAT F$="adios"
35 MAT B$="hola"
40 MAT PRINT B$;F$
45 MAT C$=B$+F$
46 MAT C$=B$-A$ 'FORBIDDEN_EXPRESION = (37, "Expression not allowed.")
50 MAT PRINT C$
55 END
60 PRINT "Error";ERR;"en línea";ERL
65 RESUME NEXT''',

'''Error 5 en línea 20
hola  hola  hola  hola  hola
hola  hola  hola  hola  hola
hola  hola  hola  hola  hola
hola  hola  hola  hola  hola
hola  hola  hola  hola  hola

adios  adios  adios  adios  adios
adios  adios  adios  adios  adios
adios  adios  adios  adios  adios
adios  adios  adios  adios  adios
adios  adios  adios  adios  adios
Error 37 en línea 46
holaadios  holaadios  holaadios  holaadios  holaadios
holaadios  holaadios  holaadios  holaadios  holaadios
holaadios  holaadios  holaadios  holaadios  holaadios
holaadios  holaadios  holaadios  holaadios  holaadios
holaadios  holaadios  holaadios  holaadios  holaadios'''    
),
(
'''10 MAT BASE 1
20 DIM A(3,3)
30 DATA -3, 2, 3, 5, -3, 5, 2, 5, -1
70 MAT READ A
90 MAT PRINT USING "##.###";A;''',

'''-3.000   2.000   3.000
 5.000  -3.000   5.000
 2.000   5.000  -1.000'''
),
(
'''10 MAT BASE 1
20 DIM A(3,3)
30 DATA -3, 2, 3, 5, -3, 5, 2, 5, -1
70 MAT READ A
90 MAT PRINT USING "0#.###";A;''',

'''-3.000  02.000  03.000
05.000  -3.000  05.000
02.000  05.000  -1.000'''
),
(
'''10 MAT BASE 1
20 DIM A(3,3)
30 DATA 1, 2, 3, 4, 5, 6, 7, 8, 9
40 FOR I=1 TO 3
50 READ A(I,1),A(I,2),A(I,3)
60 NEXT I
70 K=3 : GOSUB 130
80 REDIM A(2,2)
90 K=2 : GOSUB 130
100 REDIM A(3,3)
110 K=3 : GOSUB 130
120 END
130 FOR I=1 TO K
140 FOR J=1 TO K
150 PRINT A(I,J);
160 NEXT J
170 PRINT
180 NEXT I
190 PRINT
200 RETURN''',

''' 1  2  3 
 4  5  6 
 7  8  9 

 1  2 
 4  5 

 1  2  0 
 4  5  0 
 0  0  0 
'''    
),
(
'''10 MAT BASE 1
20 DIM c(4,3)
30 MAT c=ZER
40 k=4 : l=3 : GOSUB 100
50 REDIM c(3,2) : MAT c=CON
60 k=3 : l=2 : GOSUB 100
70 REDIM c(4,3)
80 k=4 : l=3 : GOSUB 100
90 END
100 MAT PRINT c
120 PRINT
130 RETURN''',

''' 0   0   0
 0   0   0
 0   0   0
 0   0   0

 1   1
 1   1
 1   1

 1   1   0
 1   1   0
 1   1   0
 0   0   0
'''
),
(
'''10 MAT BASE 1
20 DIM a(4,3),b(10),c(4,5)
20 DIM a(2,3),b(5,5)
30 DATA 1, 2, 3, 4, 5, 6
40 MAT READ a
50 MAT PRINT a
60 PRINT
70 MAT b=TRN(a)
80 MAT PRINT b
90 PRINT
100 MAT a=TRN(a)
110 MAT PRINT a''',

''' 1   2   3
 4   5   6

 1   4
 2   5
 3   6

 1   4
 2   5
 3   6'''
),
(
'''10 MAT BASE 1
20 DIM a(4,4)
30 MAT a=IDN
40 MAT a=2*a
50 MAT PRINT a
60 MAT a=-a
70 MAT PRINT a
80 END''',

''' 2   0   0   0
 0   2   0   0
 0   0   2   0
 0   0   0   2
-2   0   0   0
 0  -2   0   0
 0   0  -2   0
 0   0   0  -2'''
),
(
'''10 DIM a(10),b(10,10)
20 MAT a=3
30 MAT b=a
40 MAT PRINT b''',

''' 3
 3
 3
 3
 3
 3
 3
 3
 3
 3
 3'''    
),
(
'''10 MAT BASE 1 : DEG
20 DIM a(2,4),b(2,4)
30 DATA 12, 52, 76, 33, 81, 70, 72, 14
40 MAT READ a
50 MAT b=50 : b(1,2)=b(2,1)=0
60 MAT a=0.7*a : MAT b=(0.3*SIN(60))*b 'Válido porque evalúa a un solo escalar
65 MAT a=a-b
70 MAT PRINT USING " ##.#";a''',

''' -4.6   36.4   40.2   10.1
 56.7   36.0   37.4   -3.2'''
),
(
'''10 MAT BASE 1 : DEG
20 DIM a(2,4),b(2,4)
30 DATA 12, 52, 76, 33, 81, 70, 72, 14
40 MAT READ a
50 MAT b=50 : b(1,2)=b(2,1)=0
60 MAT a=0.7*a : MAT b=(0.3*b*SIN(60)) 'Solo un vector y un escalar
65 MAT a=a-b
70 MAT PRINT USING " ##.#";a''',

'''Line 60. Expression not allowed.'''
),
(
'''10 MAT BASE 1
20 DIM a(3,4),b(4,2),c(3,2)
30 DATA 25, 23, 17, 12
40 DATA 17, 13, 11, 7
50 DATA 21, 18, 12, 13
60 DATA 10, 15, 20, 27, 35, 50, 60, 80
70 MAT READ a,b
80 MAT c=a*b
90 PRINT " OLD K$   NEW K$"
100 PRINT
110 MAT PRINT USING "#,###  ";c
120 END''',

''' OLD K$   NEW K$

2,025    2,806  
1,235    1,716  
1,770    2,441  '''
),
(
'''10 MAT BASE 1
20 DIM a(3,4),b(3,2),c(4,2)
30 DATA 25, 23, 17, 12
40 DATA 17, 13, 11, 7
50 DATA 21, 18, 12, 13
60 DATA 80, 90, 75, 85, 85, 95
70 MAT READ a,b
80 MAT b=0.01*b
90 MAT a=TRN(a)
95 MAT c=a*b
100 PRINT " JUNE  JULY"
110 PRINT
120 MAT PRINT USING "##.#";c
130 END''',

''' JUNE  JULY

50.6  56.9
43.5  48.9
32.0  36.0
25.9  29.1'''
),
(
'''10 MAT BASE 1
20 DIM a(3),b(1,3)
30 DATA 1, 2, 3, 4, 5, 6
40 MAT READ a,b
45 MAT PRINT a;b
46 PRINT
50 MAT c=a*b
60 MAT PRINT c''',

''' 1
 2
 3

 4   5   6

 4   5   6
 8   10   12
 12   15   18'''
),
(
'''10 MAT BASE 0
20 DIM a(2,0),b(0,2)
30 DATA 1, 2, 3, 4, 5, 6
40 MAT READ a,b
45 MAT PRINT a;b
46 PRINT
50 MAT c=a*b
60 MAT PRINT c''',

''' 1
 2
 3

 4   5   6

 4   5   6
 8   10   12
 12   15   18'''
),
(
'''10 MAT BASE 1
20 DATA 1, 3, 5, 7, 9, 11, 13, 15, 17
30 DATA 2, 4, 6, 8, 10, 12, 14, 16, 18
40 DIM a(3,3),b(3,3)
50 MAT READ a,b
60 MAT c=a+b '5*(A+B)*(A-B)
70 MAT a=a-b
80 MAT c=5*c
90 MAT a=c*a
100 MAT PRINT a''',

'''-105  -105  -105
-285  -285  -285
-465  -465  -465'''
),
(
'''10 MAT BASE 1
15 DIM a(3,3)
20 MAT READ a
30 MAT b=INV(a)
40 MAT c=a*b
50 MAT PRINT a,b,c,
60 PRINT "Determinante=";DET(a)
70 DATA 5, 3, 1, 3, 7, 4, 1, 4, 9
80 END''',

'''                     5                     3                     1
                     3                     7                     4
                     1                     4                     9

      0.27485380116959     -0.13450292397661     0.029239766081871
     -0.13450292397661      0.25730994152047    -0.099415204678363
     0.029239766081871    -0.099415204678363      0.15204678362573

                     1   1.3877787807814E-17   2.7755575615629E-17
  -1.3877787807814E-16                     1  -1.1102230246252E-16
                     0                     0                     1
Determinante= 171'''
),
(
'''10 DATA 1, 2, 3, 4
20 MAT BASE 1
30 DIM a(1,2),b(2)
40 MAT READ a,b
50 MAT c=a*b
60 MAT d=b*a
70 MAT PRINT c;d''',

''' 11

 3   6
 4   8'''
),
(
'''10 DATA 1, 2, 3, 4, 5, 6, 7, 8, 9
20 DATA 1, 0, 2, 0, 3, 0, 4, 0, 5
30 MAT BASE 1
40 DIM a(3,3),b(3,3),c(3,3)
50 MAT READ a,b
60 MAT c=IDN
70 MAT PRINT a;b;c''',

''' 1   2   3
 4   5   6
 7   8   9

 1   0   2
 0   3   0
 4   0   5

 1   0   0
 0   1   0
 0   0   1'''
),
(
'''10 MAT BASE 1
20 DATA 1, 2, 3, 4
30 DIM a(1,2),b(2)
40 MAT READ a,b
50 MAT a=a-b
60 MAT PRINT a
70 END''',

'''Line 50. Invalid number of dimensions.'''
),
(
'''10 MAT BASE 1
20 DATA 1, 2, 3, 4, 5, 6, 7, 8
30 DIM a(4,1),b(4)
40 MAT READ a,b
50 MAT a=a-b
60 MAT PRINT a
70 END''',

'''-4
-4
-4
-4'''
),
(
'''10 MAT BASE 0
20 DIM A(1)
30 A(0)=-2/223323234 : A(1)=-123234125218/35
40 MAT PRINT A
50 PRINT A(0) : PRINT A(1)''',

'''-8.9556288621541E-09
-3520975006.2286
-8.9556288621541E-09
-3520975006.2286'''
),
(
'''10 REM Solución de serie de ecuaciones lineales
15 MAT BASE 1
20 DIM c(5,5),d(5)
30 MAT READ c,d
40 PRINT "Matriz de coeficientes:"
50 MAT PRINT USING "##";c
60 PRINT "Términos independientes:"
70 MAT PRINT d
80 MAT x=INV(c)
90 MAT x=x*d
100 PRINT "Vector solución:"
110 MAT PRINT COL x
120 MAT f=c*x
130 MAT f=d-f
140 PRINT "Vector error:"
150 MAT PRINT COL f
160 DATA 11, 3, 0, 1, 2, 0, 4, 2, 0, 1, 3, 2, 7, 1, 0
170 DATA 4, 0, 4, 10, 1, 2, 5, 1, 3, 13, 51, 15, 15, 20, 92
180 END''',

'''Matriz de coeficientes:
11   3   0   1   2
 0   4   2   0   1
 3   2   7   1   0
 4   0   4  10   1
 2   5   1   3  13
Términos independientes:
 51
 15
 15
 20
 92
Vector solución:
 2.9791651927839   2.2155995755218   0.21128404669261   0.15231694375663   5.7150336045278
Vector error:
 0  -1.2434497875802E-14  -5.3290705182008E-15  -3.5527136788005E-15   0'''
),
(
'''10 MAT BASE 1
15 DATA 1, 2, 3, 4, 5, 6, 7, 8, 17
20 DATA 10, 11, 12
25 DIM a(3,3),c(3)
30 MAT READ a,c
35 MAT b=INV(a)
40 MAT x=b*c
45 MAT PRINT x
50 x1=EXP(x(1,1))
55 PRINT x1
60 x1=EXP(x(1))
65 PRINT x1
70 END''',

'''-9.3333333333333
 9.6666666666667
 4.4408920985006E-16
 8.8426988659883E-05
 8.8426988659883E-05'''
),
(
'''10 MAT BASE 1
20 DIM R(3)
30 DATA 10, 20, 30
40 MAT READ R
50 PRINT R(1,1)
60 PRINT R(1)
70 END''',

''' 10
 10'''
),
(
'''10 MAT BASE 1
20 DIM a(3),b(1,3)
30 DATA 1, 2, 3, 4, 5, 6
40 MAT READ a,b
50 MAT x=a*b
60 MAT y=b*a
70 MAT PRINT x;y
80 PRINT y(1),y(1,1),y(0,0),y(0,1),y(1,0),y(1,2)''',

''' 4   5   6
 8   10   12
 12   15   18

 32
 32\t 32\t 0\t 0\t 0\t
Line 80. Index out of range.'''
),
('''10 DIM A(5),B(2,4),
20 IF LBOUND(A)<>0 THEN PRINT "E1": END
30 IF UBOUND(A)<>5 THEN PRINT "E2": END
40 IF LBOUND(B)<>0 THEN PRINT "E3": END
50 IF UBOUND(B)<>2 THEN PRINT "E4": END
60 IF LBOUND(B,2)<>0 THEN PRINT "E5": END
70 IF UBOUND(B,2)<>4 THEN PRINT "E6": END
80 PRINT "OK"
90 END''',

'''OK'''
),
('''10 MAT BASE 1
20 DIM C(3),D(1,2,4)
30 IF LBOUND(C)<>1 THEN PRINT "E1": END
40 IF LBOUND(D,3)<>1 THEN PRINT "E2": END
50 IF UBOUND(C)<>3 THEN PRINT "E3": END
60 IF UBOUND(D,3)<>4 THEN PRINT "E4": END
70 IF LBOUND(D)<>1 THEN PRINT "E5": END
80 IF UBOUND(D)<>1 THEN PRINT "E6": END
90 IF UBOUND(D,2)<>2 THEN PRINT "E7": END
100 PRINT "BASE1"
110 END''',

'''BASE1'''
),
('''10 A=7
20 DIM A(20)
30 A(16)=29
40 PRINT A
50 PRINT A(16)
60 PRINT LBOUND(A)
70 PRINT UBOUND(A)
80 REDIM A(10)
90 MAT BASE 1
100 PRINT LBOUND(A)
110 PRINT UBOUND(A)
120 DIM Z(1)
130 PRINT UBOUND(Z,2)''',

''' 7
 29
 0
 20
 1
 10
Line 130. Index out of range.'''
)
])

def test_basic_program(run_basic_interpreter, program_code, expected_output):
    """
    Prueba automatizada para programas BASIC usando pytest.
    """
    # Definir los comandos a enviar al intérprete
    commands = ['NEW']  # Asegurarse de que el estado esté limpio
    program_lines = program_code.strip().split('\n')
    commands.extend(program_lines)
    commands.append('RUN')   # Ejecutar el programa
    commands.append('EXIT')  # Salir del intérprete

    # Ejecutar el intérprete con los comandos
    output = run_basic_interpreter(commands)

    # Para depuración: imprimir la salida completa
    print("Salida del intérprete:\n", output)

    # Definir patrones de líneas a excluir (mensajes de bienvenida, despedida y "Ready")
    relevant_output_lines = _filtered_basic_output_lines(output)
    relevant_output = '\n'.join(relevant_output_lines)
    
    if program_code.startswith('10 AFTER 20 GOSUB 50\n20 x=REMAIN(0)\n'):
        valid_outputs = {
            expected_output,
            expected_output.replace(' 20  19', ' 21  20  19', 1),
        }
        assert relevant_output in valid_outputs, (
            f"La salida del temporizador no coincide.\n\nEsperado uno de:\n{valid_outputs}\n\nObtenido:\n{relevant_output}"
        )
        return

    # Comparar que la salida esperada está incluida en la salida obtenida
    assert expected_output == relevant_output, (
        f"La salida no coincide exactamente con el texto esperado.\n\nEsperado:\n{expected_output}\n\nObtenido:\n{relevant_output}"
    )


def _filtered_basic_output_lines(output, trim_trailing_blank=False):
    exclude_patterns = DEFAULT_SESSION_NOISE_PATTERNS

    # Filtrar las líneas relevantes usando patrones
    relevant_output_lines = []
    for raw_line in output.splitlines():
        line = raw_line.rstrip('\r')
        if not any(re.fullmatch(pattern, line) for pattern in exclude_patterns):
            relevant_output_lines.append(line)
    if trim_trailing_blank:
        while relevant_output_lines and relevant_output_lines[-1] == '':
            relevant_output_lines.pop()
    return relevant_output_lines


def test_run_supports_subdirectories_and_cd_root(tmp_path, run_basic_interpreter):
    (tmp_path / "raiz.bas").write_text('10 PRINT "RAIZ"\n', encoding='utf-8')
    examples_dir = tmp_path / "ejemplos"
    examples_dir.mkdir()
    (examples_dir / "demo.bas").write_text('10 PRINT "SUBDIR"\n', encoding='utf-8')

    output = run_basic_interpreter([
        'RUN "ejemplos/demo.bas"',
        'CD "ejemplos"',
        'RUN "demo.bas"',
        'CD "/"',
        'RUN "raiz.bas"',
        'EXIT',
    ], cwd=tmp_path)

    assert _filtered_basic_output_lines(output, trim_trailing_blank=True) == ['SUBDIR', 'SUBDIR', 'RAIZ']


def test_files_follow_virtual_current_directory_and_cd_parent(tmp_path, run_basic_interpreter):
    (tmp_path / "raiz.bas").write_text('10 PRINT "RAIZ"\n', encoding='utf-8')
    examples_dir = tmp_path / "ejemplos"
    examples_dir.mkdir()
    (examples_dir / "demo.bas").write_text('10 PRINT "SUBDIR"\n', encoding='utf-8')

    output = run_basic_interpreter([
        'FILES "*.bas"',
        'CD "ejemplos"',
        'FILES "*.bas"',
        'CD ".."',
        'FILES "*.bas"',
        'EXIT',
    ], cwd=tmp_path)

    listed_files = re.findall(r'[\w.-]+\.bas', '\n'.join(_filtered_basic_output_lines(output, trim_trailing_blank=True)))
    assert listed_files == ['raiz.bas', 'demo.bas', 'raiz.bas']


def test_files_lists_subdirectories_with_trailing_slash(tmp_path, run_basic_interpreter):
    examples_dir = tmp_path / "ejemplos"
    examples_dir.mkdir()

    output = run_basic_interpreter([
        'FILES "*.bas"',
        'EXIT',
    ], cwd=tmp_path)

    lines = _filtered_basic_output_lines(output, trim_trailing_blank=True)
    assert any('ejemplos/' in line for line in lines)


def test_cd_parent_is_clamped_at_virtual_root(tmp_path, run_basic_interpreter):
    (tmp_path / "raiz.bas").write_text('10 PRINT "RAIZ"\n', encoding='utf-8')

    output = run_basic_interpreter([
        'CD ".."',
        'RUN "raiz.bas"',
        'EXIT',
    ], cwd=tmp_path)

    assert _filtered_basic_output_lines(output, trim_trailing_blank=True) == ['RAIZ']


def test_running_file_does_not_change_current_directory(tmp_path, run_basic_interpreter):
    (tmp_path / "raiz.bas").write_text('10 PRINT "RAIZ"\n', encoding='utf-8')
    samples_dir = tmp_path / "samples"
    samples_dir.mkdir()
    (samples_dir / "demo.bas").write_text('10 PRINT "DEMO"\n', encoding='utf-8')

    output = run_basic_interpreter([
        'RUN "samples/demo.bas"',
        'FILES "*.bas"',
        'EXIT',
    ], cwd=tmp_path)

    lines = _filtered_basic_output_lines(output, trim_trailing_blank=True)
    assert lines[0] == 'DEMO'
    listed_files = re.findall(r'[\w.-]+\.bas', '\n'.join(lines[1:]))
    assert listed_files == ['raiz.bas']


def test_program_dir_is_used_for_chain_inside_loaded_program(tmp_path, run_basic_interpreter):
    samples_dir = tmp_path / "samples"
    samples_dir.mkdir()
    (samples_dir / "parent.bas").write_text('10 CHAIN "child.bas"\n', encoding='utf-8')
    (samples_dir / "child.bas").write_text('10 PRINT "CHILD"\n', encoding='utf-8')

    output = run_basic_interpreter([
        'RUN "samples/parent.bas"',
        'EXIT',
    ], cwd=tmp_path)

    assert _filtered_basic_output_lines(output, trim_trailing_blank=True) == ['CHILD']


def test_load_and_run_ignore_blank_lines_in_source_file(tmp_path, run_basic_interpreter):
    (tmp_path / "blank.bas").write_text('10 PRINT "A"\n\n20 PRINT "B"\n\n', encoding='utf-8')

    output = run_basic_interpreter([
        'LOAD "blank.bas"',
        'RUN',
        'EXIT',
    ], cwd=tmp_path)

    lines = _filtered_basic_output_lines(output, trim_trailing_blank=True)
    assert lines == ['A', 'B']
    assert "Invalid line format." not in output


def test_png_resolution_uses_program_dir_when_running_program(tmp_path):
    interpreter = BasicInterpreter()
    interpreter.root_dir = tmp_path.resolve()
    interpreter.current_dir = interpreter.root_dir

    root_assets = tmp_path / "assets"
    root_assets.mkdir()
    root_png = root_assets / "sprite.png"
    root_png.write_bytes(b'')

    samples_assets = tmp_path / "samples" / "assets"
    samples_assets.mkdir(parents=True)
    samples_png = samples_assets / "sprite.png"
    samples_png.write_bytes(b'')

    interpreter.program_dir = (tmp_path / "samples").resolve()

    assert Path(interpreter.resolve_png_filename('"assets/sprite.png"', is_program=False)).resolve() == root_png.resolve()
    assert Path(interpreter.resolve_png_filename('"assets/sprite.png"', is_program=True)).resolve() == samples_png.resolve()


def test_print_parenthesized_comma_reports_syntax_error(run_basic_interpreter):
    output = run_basic_interpreter([
        'PRINT (2,2)',
        'EXIT',
    ])
    assert "Syntax error." in output


@pytest.mark.parametrize("statement", [
    "PRINT 2()",
    "PRINT 2(4)",
    "PRINT 2( 2)",
    "PRINT ()",
    "PRINT ()-5",
    "PRINT 5-()",
])
def test_print_number_call_reports_syntax_error(run_basic_interpreter, statement):
    output = run_basic_interpreter([
        statement,
        'EXIT',
    ])
    assert "Syntax error." in output
    assert "SyntaxWarning" not in output
    assert "Invalid value type." not in output


def test_string_square_bracket_access_is_one_based():
    interpreter = BasicInterpreter()
    interpreter.variables['A$'] = 'HOLA'

    assert interpreter.evaluate_expression('A$[1]') == 'H'
    assert interpreter.evaluate_expression('A$[3]') == 'L'


def test_string_square_bracket_access_supports_string_array_elements(run_basic_interpreter):
    output = run_basic_interpreter([
        'NEW',
        '10 DIM A$(2)',
        '20 A$(1)="HOLA"',
        '30 PRINT A$(1)[3]',
        '40 END',
        'RUN',
        'EXIT',
    ])
    assert _filtered_basic_output_lines(output, trim_trailing_blank=True) == ['L']


@pytest.mark.parametrize(
    ("statement", "message"),
    [
        ('PRINT "HOLA"[2]', 'Syntax error.'),
        ('PRINT A$[1:2]', 'Syntax error.'),
        ('PRINT [1,2]', 'Syntax error.'),
        ('PRINT "hola".upper()', 'Syntax error.'),
        ('PRINT A$[0]', 'Invalid index.'),
        ('PRINT A$[-1]', 'Invalid index.'),
        ('PRINT A$[1.5]', 'Invalid index.'),
        ('PRINT A$[5]', 'Index out of range.'),
        ('PRINT N[1]', 'Invalid value type.'),
    ],
)
def test_string_square_bracket_access_rejects_invalid_forms(run_basic_interpreter, statement, message):
    output = run_basic_interpreter([
        'NEW',
        '10 A$="HOLA":N=123',
        f'20 {statement}',
        '30 END',
        'RUN',
        'EXIT',
    ])
    assert message in output


def test_renum_with_third_parameter_renumbers_only_tail_block(run_basic_interpreter):
    output = run_basic_interpreter([
        'NEW',
        '10 GOSUB 500',
        '20 END',
        '500 GOSUB 20',
        '510 RETURN',
        'RENUM 1000,10,500',
        'LIST',
        'EXIT',
    ])

    assert _filtered_basic_output_lines(output, trim_trailing_blank=True) == [
        '10 GOSUB 1000',
        '20 END',
        '1000 GOSUB 20',
        '1010 RETURN',
    ]


def test_renum_with_third_parameter_rejects_incompatible_numbering(run_basic_interpreter):
    output = run_basic_interpreter([
        'NEW',
        '10 PRINT "A"',
        '20 GOSUB 500',
        '30 END',
        '500 PRINT "S"',
        '510 RETURN',
        'RENUM 25,10,500',
        'LIST',
        'EXIT',
    ])

    assert "Invalid argument." in output
    lines = _filtered_basic_output_lines(output, trim_trailing_blank=True)
    assert lines[0] == 'Invalid argument.'
    assert lines[1:] == [
        '10 PRINT "A"',
        '20 GOSUB 500',
        '30 END',
        '500 PRINT "S"',
        '510 RETURN',
    ]


def test_line_input_allows_commas_without_quotes(run_basic_interpreter):
    commands = [
        'NEW',
        '10 LINE INPUT "Texto"; A$',
        '20 PRINT A$',
        '30 END',
        'RUN',
        'uno, dos, tres',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)

    exclude_patterns = DEFAULT_SESSION_NOISE_PATTERNS

    relevant_output_lines = []
    for raw_line in output.splitlines():
        line = raw_line.rstrip('\\r')
        if not any(re.fullmatch(pattern, line) for pattern in exclude_patterns):
            relevant_output_lines.append(line)

    while relevant_output_lines and relevant_output_lines[-1] == '':
        relevant_output_lines.pop()

    assert relevant_output_lines == ['Texto? uno, dos, tres']


def test_line_input_requires_string_target(run_basic_interpreter):
    commands = [
        'NEW',
        '10 LINE INPUT "Dato"; N',
        '20 END',
        'RUN',
        '123,456',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert "Invalid value type" in output


def test_input_preserves_internal_quotes(run_basic_interpreter):
    commands = [
        'NEW',
        '10 INPUT A$',
        '20 PRINT A$',
        '30 END',
        'RUN',
        'Esto es un "ejemplo" de cadena válida',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)

    exclude_patterns = DEFAULT_SESSION_NOISE_PATTERNS

    relevant_output_lines = []
    for raw_line in output.splitlines():
        line = raw_line.rstrip('\r')
        if not any(re.fullmatch(pattern, line) for pattern in exclude_patterns):
            relevant_output_lines.append(line)

    while relevant_output_lines and relevant_output_lines[-1] == '':
        relevant_output_lines.pop()

    assert relevant_output_lines == [
        '? Esto es un "ejemplo" de cadena válida',
    ]


def test_input_with_outer_quotes_keeps_internal_quotes(run_basic_interpreter):
    commands = [
        'NEW',
        '10 INPUT A$',
        '20 PRINT A$',
        '30 END',
        'RUN',
        '"Esto es un "ejemplo" de cadena válida"',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)

    exclude_patterns = DEFAULT_SESSION_NOISE_PATTERNS

    relevant_output_lines = []
    for raw_line in output.splitlines():
        line = raw_line.rstrip('\r')
        if not any(re.fullmatch(pattern, line) for pattern in exclude_patterns):
            relevant_output_lines.append(line)

    while relevant_output_lines and relevant_output_lines[-1] == '':
        relevant_output_lines.pop()

    assert relevant_output_lines == [
        '? Esto es un "ejemplo" de cadena válida',
    ]


def test_line_input_preserves_internal_quotes(run_basic_interpreter):
    commands = [
        'NEW',
        '10 LINE INPUT "Texto"; A$',
        '20 PRINT A$',
        '30 END',
        'RUN',
        'Esto es un "ejemplo" de cadena válida',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)

    exclude_patterns = DEFAULT_SESSION_NOISE_PATTERNS

    relevant_output_lines = []
    for raw_line in output.splitlines():
        line = raw_line.rstrip('\r')
        if not any(re.fullmatch(pattern, line) for pattern in exclude_patterns):
            relevant_output_lines.append(line)

    while relevant_output_lines and relevant_output_lines[-1] == '':
        relevant_output_lines.pop()

    assert relevant_output_lines == [
        'Texto? Esto es un "ejemplo" de cadena válida',
    ]


def test_mat_input_populates_arrays(run_basic_interpreter):
    commands = [
        'NEW',
        '10 MAT BASE 1',
        '20 DIM R(3), S(2,2), T$(2)',
        '25 X=10',
        '30 MAT INPUT R, S',
        '40 MAT BASE 0',
        '50 MAT INPUT T$',
        '60 MAT PRINT COL R;',
        '70 MAT PRINT ROW S;',
        '80 MAT PRINT COL T$;',
        '90 END',
        'RUN',
        '1, X, 3, 4, 5',
        'SIN(0), 8',
        '"UNO","DOS"',
        '',
    ]

    output = run_basic_interpreter(commands)

    exclude_patterns = DEFAULT_SESSION_NOISE_PATTERNS

    relevant_output_lines = []
    for raw_line in output.splitlines():
        line = raw_line.rstrip('\r')
        if not any(re.fullmatch(pattern, line) for pattern in exclude_patterns):
            relevant_output_lines.append(line)

    while relevant_output_lines and relevant_output_lines[-1] == '':
        relevant_output_lines.pop()

    expected_output = [
        'R(1)? S(2,1)? T$(0)? T$(2)?  0   1   10   3',
        ' 0   0   0',
        ' 0   4   5',
        ' 0   0   8',
        'UNO  DOS  ',
    ]

    assert relevant_output_lines == expected_output

def test_mat_input_reprompts_after_error(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DIM r(3)',
        '20 MAT INPUT r',
        '30 MAT PRINT ROW r;',
        '40 END',
        'RUN',
        '3,4/0,2,6',
        '5,7,8',
        '',
    ]

    output = run_basic_interpreter(commands)

    exclude_patterns = DEFAULT_SESSION_NOISE_PATTERNS

    relevant_output_lines = []
    for raw_line in output.splitlines():
        line = raw_line.rstrip('\r')
        if not any(re.fullmatch(pattern, line) for pattern in exclude_patterns):
            relevant_output_lines.append(line)

    while relevant_output_lines and relevant_output_lines[-1] == '':
        relevant_output_lines.pop()

    expected_output = [
        'r(0)? Line 20. Division by zero.',
        'r(1)?  3',
        ' 5',
        ' 7',
        ' 8'
    ]

    assert relevant_output_lines == expected_output

def test_mat_input_on_error_resume_next(run_basic_interpreter):
    commands = [
        'NEW',
        '10 MAT BASE 1',
        '15 ON ERROR GOTO 100',
        '20 DIM miPRUeba(3), S(2,2), T$(2)',
        '25 X=10',
        '30 MAT INPUT miPRUeba, S',
        '40 MAT BASE 0',
        '50 MAT INPUT T$',
        '60 MAT PRINT COL miPRUeba',
        '70 MAT PRINT ROW S',
        '80 MAT PRINT COL T$',
        '90 END',
        '100 RESUME NEXT',
        'RUN',
        '3,4/0,5',
        '1,2',
    ]

    output = run_basic_interpreter(commands)

    exclude_patterns = DEFAULT_SESSION_NOISE_PATTERNS

    relevant_output_lines = []
    for raw_line in output.splitlines():
        line = raw_line.rstrip('\r')
        if not any(re.fullmatch(pattern, line) for pattern in exclude_patterns):
            relevant_output_lines.append(line)

    while relevant_output_lines and relevant_output_lines[-1] == '':
        relevant_output_lines.pop()

    expected_output = [
        'miPRUeba(1)? T$(0)?  0   3   0   0',
        ' 0   0   0',
        ' 0   0   0',
        ' 0   0   0',
        '    '
    ]

    assert relevant_output_lines == expected_output


def test_on_error_resume_next_shorthand_continues_execution(run_basic_interpreter):
    commands = [
        'NEW',
        '10 ON ERROR RESUME NEXT',
        '20 PRINT "INI"',
        '30 X=1/0 : PRINT "MISMA"',
        '40 Y=1/0',
        '50 PRINT "SIG"',
        '60 END',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert "Division by zero." not in output

    exclude_patterns = DEFAULT_SESSION_NOISE_PATTERNS

    relevant_output_lines = []
    for raw_line in output.splitlines():
        line = raw_line.rstrip('\r')
        if not any(re.fullmatch(pattern, line) for pattern in exclude_patterns):
            relevant_output_lines.append(line)

    while relevant_output_lines and relevant_output_lines[-1] == '':
        relevant_output_lines.pop()

    assert relevant_output_lines == ['INI', 'MISMA', 'SIG']


def test_on_error_goto_0_disables_resume_next_shorthand(run_basic_interpreter):
    commands = [
        'NEW',
        '10 ON ERROR RESUME NEXT',
        '20 ON ERROR GOTO 0',
        '30 X=1/0',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert "Line 30. Division by zero." in output


def test_mat_input_string_retry_after_error(run_basic_interpreter):
    commands = [
        'NEW',
        '10 MAT BASE 1',
        '20 DIM miPRUeba(3), S(2,2), T$(2)',
        '25 X=10',
        '30 MAT INPUT miPRUeba, S',
        '40 MAT BASE 0',
        '50 MAT INPUT T$',
        '60 MAT PRINT COL miPRUeba',
        '70 MAT PRINT ROW S',
        '80 MAT PRINT COL T$',
        '90 END',
        'RUN',
        '1,2,3',
        '1,2,3,4',
        '1,2,3',
        '"1-1","2-2","3-3"',
        '',
    ]

    output = run_basic_interpreter(commands)

    exclude_patterns = DEFAULT_SESSION_NOISE_PATTERNS

    relevant_output_lines = []
    for raw_line in output.splitlines():
        line = raw_line.rstrip('\r')
        if not any(re.fullmatch(pattern, line) for pattern in exclude_patterns):
            relevant_output_lines.append(line)

    while relevant_output_lines and relevant_output_lines[-1] == '':
        relevant_output_lines.pop()

    expected_output = [
        'miPRUeba(1)? S(1,1)? T$(0)? Line 50. Missing quotes.',
        'T$(0)?  0   1   2   3',
        ' 0   0   0',
        ' 0   1   2',
        ' 0   3   4',
        '1-1  2-2  3-3'
    ]

    assert relevant_output_lines == expected_output

def test_mat_input_large_program_example(run_basic_interpreter):
    commands = [
        'NEW',
        '10 MAT BASE 1',
        '20 DIM R(3),S(5,4),T(2,3)',
        '30 MAT INPUT R,S',
        '40 X=2 : Y=5',
        '50 MAT INPUT T',
        '60 MAT PRINT R;S;T',
        '70 END',
        'RUN',
        '1.5',
        '2.5',
        '3.5,101,102,103,104',
        '201,202,203,204,301,302,303,304,401,402,403,404,501',
        '502,503,504',
        'x,sin(x),1-cos(2*x)',
        'y,cos(y),1-sin(2*y)',
        '',
    ]

    output = run_basic_interpreter(commands)

    exclude_patterns = DEFAULT_SESSION_NOISE_PATTERNS

    relevant_output_lines = []
    for raw_line in output.splitlines():
        line = raw_line.rstrip('\r')
        if not any(re.fullmatch(pattern, line) for pattern in exclude_patterns):
            relevant_output_lines.append(line)

    while relevant_output_lines and relevant_output_lines[-1] == '':
        relevant_output_lines.pop()

    expected_output = [
        'R(1)? R(2)? R(3)? S(2,1)? S(5,2)? T(1,1)? T(2,1)?  1.5',
        ' 2.5',
        ' 3.5',
        '',
        ' 101   102   103   104',
        ' 201   202   203   204',
        ' 301   302   303   304',
        ' 401   402   403   404',
        ' 501   502   503   504',
        '',
        ' 2   0.90929742682568   1.6536436208636',
        ' 5   0.28366218546323   1.5440211108894',
    ]

    assert relevant_output_lines == expected_output

class SpriteTestWindow(GraphicsWindow):
    def __init__(self, width=10, height=10, fill="#000000"):
        fill_value = GraphicsWindow._resolve_color(fill)
        self.width = width
        self.height = height
        self.origin_x = 0
        self.origin_y = 0
        self.intersection_x = 0
        self.intersection_y = 0
        self.cross_at_x = None
        self.cross_at_y = None
        self.reset_scale()
        self.w_left = 0
        self.w_right = width - 1
        self.w_top = 0
        self.w_bottom = height - 1
        self.buffer = [fill_value] * (width * height)
        self.dirty_grid = DirtyGrid(width, height, max(width, height) or 1)
        self.current_color = GraphicsWindow._resolve_color("#ffffff")
        self.background_color = fill_value
        self.mask = 255
        self.pen_width = 1
        self.bitmap_font = small_bitmap_font
        self.ldir = 0
        self.cursor_x = 0
        self.cursor_y = 0
        self.cursor_user_x = 0
        self.cursor_user_y = 0
        self.cursor_history = []
        self._sprite_runs_cache = {}
        self.collision_mode = GraphicsWindow.COLLISION_OFF
        self.collision_color_filter = None
        self.collision_last_any = False
        self.collision_last_color = False
        self.collision_last_sprite = False
        self.collision_last_color_rgb = None
        self.collision_last_sprite_id = 0
        self._sprite_owner_buffer = [0] * (width * height)
        self._sprite_owner_pixels_by_id = {}
        self._sprite_draw_counter = 0
        self._sprite_registry = {}

    def move_cursor(self, x, y):
        self.cursor_history.append((x, y))

    def _rgb_hex(self, color: str) -> str:
        return color.lstrip("#").lower()


class ScaleCursorTestWindow(SpriteTestWindow):
    def move_cursor(self, x, y):
        GraphicsWindow.move_cursor(self, x, y)


class AxisNoLabelWindow(ScaleCursorTestWindow):
    def __init__(self, width=10, height=10, fill="#000000"):
        super().__init__(width=width, height=height, fill=fill)
        self.gprint_calls = 0

    def gprint(self, *args, **kwargs):
        self.gprint_calls += 1


class AxisLabelFontWindow(ScaleCursorTestWindow):
    def __init__(self, width=10, height=10, fill="#000000"):
        super().__init__(width=width, height=height, fill=fill)
        self.fonts_seen = []

    def gprint(self, *args, **kwargs):
        self.fonts_seen.append(self.bitmap_font)


class AxisLabelTextWindow(ScaleCursorTestWindow):
    def __init__(self, width=10, height=10, fill="#000000"):
        super().__init__(width=width, height=height, fill=fill)
        self.labels_seen = []

    def gprint(self, text, *args, **kwargs):
        self.labels_seen.append(str(text))


def test_draw_sprite_renders_expected_pixels():
    GraphicsWindow._decode_sprite.cache_clear()
    gw = SpriteTestWindow(width=5, height=5)
    sprite = "3x2:" + "".join([
        "112233",
        "aabbcc",
        "ddeeff",
        "445566",
        "778899",
        "000000",
    ])

    gw.draw_sprite(sprite, 1, 1)

    zero = GraphicsWindow._resolve_color("#000000")
    expected = [zero] * (5 * 5)
    expected[2 * 5 + 1] = GraphicsWindow._resolve_color("#112233")
    expected[2 * 5 + 2] = GraphicsWindow._resolve_color("#aabbcc")
    expected[2 * 5 + 3] = GraphicsWindow._resolve_color("#ddeeff")
    expected[3 * 5 + 1] = GraphicsWindow._resolve_color("#445566")
    expected[3 * 5 + 2] = GraphicsWindow._resolve_color("#778899")
    expected[3 * 5 + 3] = zero

    assert gw.buffer == expected
    assert gw.cursor_history[-1] == (1, 1)
    assert gw.dirty_grid.dirty_blocks


def test_draw_sprite_transparent_color_zero():
    GraphicsWindow._decode_sprite.cache_clear()
    fill_color = "#123456"
    gw = SpriteTestWindow(width=4, height=4, fill=fill_color)
    sprite = "2x2:" + "".join([
        "ff0000",
        "000000",
        "00ff00",
        "0000ff",
    ])

    fill_value = GraphicsWindow._resolve_color(fill_color)
    expected = list(gw.buffer)
    expected[2 * 4 + 0] = GraphicsWindow._resolve_color("#ff0000")
    expected[2 * 4 + 1] = fill_value
    expected[3 * 4 + 0] = GraphicsWindow._resolve_color("#00ff00")
    expected[3 * 4 + 1] = GraphicsWindow._resolve_color("#0000ff")

    gw.draw_sprite(sprite, 0, 0, transparent=0)

    assert gw.buffer == expected
    assert gw.cursor_history[-1] == (0, 0)
    assert gw.dirty_grid.dirty_blocks


def test_draw_sprite_reuses_cached_decoding():
    GraphicsWindow._decode_sprite.cache_clear()
    gw = SpriteTestWindow(width=4, height=4)
    sprite = "1x1:abcdef"

    gw.draw_sprite(sprite, 0, 0)
    info_after_first_draw = GraphicsWindow._decode_sprite.cache_info()

    gw.draw_sprite(sprite, 1, 0)
    info_after_second_draw = GraphicsWindow._decode_sprite.cache_info()

    assert info_after_second_draw.hits == info_after_first_draw.hits + 1
    assert info_after_second_draw.misses == info_after_first_draw.misses


def test_sprite_collision_color_detects_non_background():
    gw = SpriteTestWindow(width=5, height=5, fill="#000000")
    gw.set_collision_mode(GraphicsWindow.COLLISION_COLOR)

    sx, sy = gw._transform_graphic(2, 2)
    gw.buffer[gw._index(sx, sy)] = GraphicsWindow._resolve_color("#ffffff")

    gw.draw_sprite("1x1:ff0000", 2, 2)

    assert gw.collision_last_any is True
    assert gw.collision_last_color is True
    assert gw.collision_last_sprite is False
    assert GraphicsWindow._get_rgb_number(gw.collision_last_color_rgb) == GraphicsWindow._get_rgb_number("#ffffff")


def test_sprite_collision_color_filter_only_matches_selected_color():
    gw = SpriteTestWindow(width=5, height=5, fill="#000000")
    gw.set_collision_mode(GraphicsWindow.COLLISION_COLOR)

    sx, sy = gw._transform_graphic(1, 1)
    gw.buffer[gw._index(sx, sy)] = GraphicsWindow._resolve_color("#ffffff")

    gw.set_collision_color("#00ff00")
    gw.draw_sprite("1x1:ff0000", 1, 1)
    assert gw.collision_last_any is False
    assert gw.collision_last_color is False

    gw.buffer[gw._index(sx, sy)] = GraphicsWindow._resolve_color("#ffffff")
    gw.set_collision_color("#ffffff")
    gw.draw_sprite("1x1:ff0000", 1, 1)
    assert gw.collision_last_any is True
    assert gw.collision_last_color is True


def test_sprite_collision_sprite_detects_overlap_and_reports_id():
    gw = SpriteTestWindow(width=5, height=5, fill="#000000")
    gw.set_collision_mode(GraphicsWindow.COLLISION_SPRITE)

    gw.draw_sprite("1x1:00ff00", 3, 3)
    first_id = gw._sprite_draw_counter
    assert gw.collision_last_sprite is False
    assert gw.collision_last_sprite_id == 0

    gw.draw_sprite("1x1:ff0000", 3, 3)
    assert gw.collision_last_any is True
    assert gw.collision_last_sprite is True
    assert gw.collision_last_sprite_id == first_id


def test_sprite_collision_with_explicit_ids_reports_other_explicit_id():
    gw = SpriteTestWindow(width=5, height=5, fill="#000000")
    gw.set_collision_mode(GraphicsWindow.COLLISION_SPRITE)

    gw.draw_sprite("1x1:00ff00", 2, 2, sprite_id=17)
    gw.draw_sprite("1x1:ff0000", 2, 2, sprite_id=42)

    assert gw.collision_last_any is True
    assert gw.collision_last_sprite is True
    assert gw.collision_last_sprite_id == 17


def test_sprite_collision_does_not_overwrite_other_sprite_pixels():
    gw = SpriteTestWindow(width=5, height=5, fill="#000000")
    gw.set_collision_mode(GraphicsWindow.COLLISION_SPRITE)

    gw.draw_sprite("1x1:00ff00", 2, 2, sprite_id=17)
    gw.draw_sprite("1x1:ff0000", 2, 2, sprite_id=42)

    sx, sy = gw._transform_graphic(2, 2)
    idx = gw._index(sx, sy)
    assert gw.buffer[idx] == GraphicsWindow._resolve_color("#00ff00")
    assert gw._sprite_owner_buffer[idx] == 17
    assert gw.collision_last_sprite is True
    assert gw.collision_last_sprite_id == 17


def test_sprite_collision_redraw_with_same_explicit_id_clears_old_ownership():
    gw = SpriteTestWindow(width=6, height=6, fill="#000000")
    gw.set_collision_mode(GraphicsWindow.COLLISION_SPRITE)

    gw.draw_sprite("1x1:00ff00", 1, 1, sprite_id=7)
    sx_old, sy_old = gw._transform_graphic(1, 1)
    old_idx = gw._index(sx_old, sy_old)
    assert gw._sprite_owner_buffer[old_idx] == 7

    gw.draw_sprite("1x1:00ff00", 2, 1, sprite_id=7)
    sx_new, sy_new = gw._transform_graphic(2, 1)
    new_idx = gw._index(sx_new, sy_new)
    assert gw._sprite_owner_buffer[old_idx] == 0
    assert gw._sprite_owner_buffer[new_idx] == 7

    gw.draw_sprite("1x1:ff0000", 1, 1, sprite_id=9)
    assert gw.collision_last_sprite is False
    assert gw.collision_last_sprite_id == 0


def test_sprite_collision_with_same_explicit_id_is_ignored_as_self_overlap():
    gw = SpriteTestWindow(width=5, height=5, fill="#000000")
    gw.set_collision_mode(GraphicsWindow.COLLISION_SPRITE)

    gw.draw_sprite("1x1:00ff00", 2, 2, sprite_id=7)
    gw.draw_sprite("1x1:ff0000", 2, 2, sprite_id=7)

    assert gw.collision_last_any is False
    assert gw.collision_last_sprite is False
    assert gw.collision_last_sprite_id == 0


def test_sprite_hittest_detects_collision_without_drawing():
    gw = SpriteTestWindow(width=5, height=5, fill="#000000")
    gw.set_collision_mode(GraphicsWindow.COLLISION_SPRITE)

    gw.draw_sprite("1x1:00ff00", 2, 2, sprite_id=17)
    sx, sy = gw._transform_graphic(2, 2)
    idx = gw._index(sx, sy)
    original_color = gw.buffer[idx]
    original_owner = gw._sprite_owner_buffer[idx]

    gw.hittest_sprite("1x1:ff0000", 2, 2, sprite_id=42)

    assert gw.collision_last_any is True
    assert gw.collision_last_sprite is True
    assert gw.collision_last_sprite_id == 17
    assert gw.buffer[idx] == original_color
    assert gw._sprite_owner_buffer[idx] == original_owner


def test_sprite_delete_removes_owner_id_from_collision_map():
    gw = SpriteTestWindow(width=5, height=5, fill="#000000")
    gw.set_collision_mode(GraphicsWindow.COLLISION_SPRITE)

    gw.draw_sprite("1x1:00ff00", 2, 2, sprite_id=17)
    sx, sy = gw._transform_graphic(2, 2)
    idx = gw._index(sx, sy)
    assert gw._sprite_owner_buffer[idx] == 17

    gw.delete_sprite(17)
    assert gw._sprite_owner_buffer[idx] == 0

    gw.hittest_sprite("1x1:ff0000", 2, 2, sprite_id=42)
    assert gw.collision_last_sprite is False
    assert gw.collision_last_sprite_id == 0


def test_sprite_redraw_with_same_id_outside_view_clears_previous_ownership():
    gw = SpriteTestWindow(width=6, height=6, fill="#000000")
    gw.set_collision_mode(GraphicsWindow.COLLISION_SPRITE)

    gw.draw_sprite("1x1:00ff00", 1, 1, sprite_id=7)
    sx_old, sy_old = gw._transform_graphic(1, 1)
    old_idx = gw._index(sx_old, sy_old)
    assert gw._sprite_owner_buffer[old_idx] == 7

    gw.draw_sprite("1x1:00ff00", 999, 999, sprite_id=7)
    assert gw._sprite_owner_buffer[old_idx] == 0
    assert gw._sprite_owner_pixels_by_id.get(7) == []


def test_sprite_move_uses_registered_sprite_and_updates_owner_map():
    gw = SpriteTestWindow(width=6, height=6, fill="#000000")
    gw.set_collision_mode(GraphicsWindow.COLLISION_SPRITE)

    gw.draw_sprite("1x1:00ff00", 1, 1, sprite_id=7)
    sx_old, sy_old = gw._transform_graphic(1, 1)
    old_idx = gw._index(sx_old, sy_old)
    assert gw._sprite_owner_buffer[old_idx] == 7

    gw.move_sprite(7, 2, 1, use_stored_transparent=True)
    sx_new, sy_new = gw._transform_graphic(2, 1)
    new_idx = gw._index(sx_new, sy_new)
    assert gw._sprite_owner_buffer[old_idx] == 0
    assert gw._sprite_owner_buffer[new_idx] == 7


def test_sprite_color_collision_ignores_self_with_explicit_id_even_after_mode_switch():
    gw = SpriteTestWindow(width=5, height=5, fill="#000000")
    gw.set_collision_mode(GraphicsWindow.COLLISION_COLOR)
    gw.set_collision_color("#ffffff")

    gw.draw_sprite("1x1:ffffff", 2, 2, sprite_id=11)
    gw.set_collision_mode(GraphicsWindow.COLLISION_SPRITE)
    gw.set_collision_mode(GraphicsWindow.COLLISION_COLOR)
    gw.set_collision_color("#ffffff")
    gw.draw_sprite("1x1:ffffff", 2, 2, sprite_id=11)

    assert gw.collision_last_any is False
    assert gw.collision_last_color is False
    assert gw.collision_last_color_rgb is None

    gw.draw_sprite("1x1:ffffff", 2, 2, sprite_id=22)
    assert gw.collision_last_any is True
    assert gw.collision_last_color is True
    assert GraphicsWindow._get_rgb_number(gw.collision_last_color_rgb) == GraphicsWindow._get_rgb_number("#ffffff")


def test_collision_commands_and_functions_in_interpreter():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=6, height=6)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("COLMODE 1")
    sx, sy = gw._transform_graphic(2, 2)
    gw.buffer[gw._index(sx, sy)] = GraphicsWindow._resolve_color("#abcdef")
    interpreter.execute_command('SPRITE "1x1:ff0000",2,2')

    assert interpreter.evaluate_expression("HIT") == -1
    assert interpreter.evaluate_expression("HITCOLOR") == GraphicsWindow._get_rgb_number("#abcdef")
    assert interpreter.evaluate_expression("HITSPRITE") == 0

    interpreter.execute_command("COLRESET")
    assert interpreter.evaluate_expression("HIT") == 0
    assert interpreter.evaluate_expression("HITCOLOR") == 0
    assert interpreter.evaluate_expression("HITID") == 0


def test_sprite_command_with_explicit_id_reports_hitid(run_basic_interpreter):
    commands = [
        'NEW',
        '10 SCREEN:CLG',
        '20 COLMODE 2',
        '30 SPRITE "1x1:00ff00",10,10,0,12',
        '40 SPRITE "1x1:ff0000",10,10,0,99',
        '50 PRINT HIT;HITSPRITE;HITID',
        'RUN',
        'EXIT',
    ]
    output = run_basic_interpreter(commands)
    assert "-1 -1  12" in output or "-1 -1 12" in output


def test_sprite_command_rejects_non_positive_explicit_id(run_basic_interpreter):
    commands = [
        'NEW',
        '10 SCREEN:CLG',
        '20 COLMODE 2',
        '30 SPRITE "1x1:00ff00",10,10,0,0',
        'RUN',
        'EXIT',
    ]
    output = run_basic_interpreter(commands)
    assert "Line 30. Invalid argument." in output


def test_sprite_hittest_and_del_commands_in_interpreter():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=6, height=6)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("COLMODE 2")
    interpreter.execute_command('SPRITE "1x1:00ff00",2,2,0,10')
    interpreter.execute_command('SPRITE HITTEST "1x1:ff0000",2,2,0,20')
    assert interpreter.evaluate_expression("HITSPRITE") == -1
    assert interpreter.evaluate_expression("HITID") == 10

    interpreter.execute_command("SPRITE DEL 10")
    interpreter.execute_command('SPRITE HITTEST "1x1:ff0000",2,2,0,20')
    assert interpreter.evaluate_expression("HITSPRITE") == 0
    assert interpreter.evaluate_expression("HITID") == 0


def test_sprite_move_command_reuses_last_sprite_for_id():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=6, height=6)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("COLMODE 2")
    interpreter.execute_command('SPRITE "1x1:00ff00",1,1,0,10')
    sx_old, sy_old = gw._transform_graphic(1, 1)
    old_idx = gw._index(sx_old, sy_old)
    assert gw._sprite_owner_buffer[old_idx] == 10

    interpreter.execute_command("SPRITE MOVE 10,2,1")
    sx_new, sy_new = gw._transform_graphic(2, 1)
    new_idx = gw._index(sx_new, sy_new)
    assert gw._sprite_owner_buffer[old_idx] == 0
    assert gw._sprite_owner_buffer[new_idx] == 10


def test_capture_sprite_pads_left_when_offscreen():
    gw = SpriteTestWindow(width=4, height=2)
    gw.buffer = [
        "#010101", "#020202", "#030303", "#040404",
        "#111111", "#222222", "#333333", "#444444",
    ]

    sprite = gw.capture_sprite(-1, 0, 1, 0)

    assert sprite == "3x1:000000111111222222"


def test_capture_sprite_inserts_black_rows_above_viewport():
    gw = SpriteTestWindow(width=2, height=2)
    gw.buffer = [
        "#aaaaaa", "#bbbbbb",
        "#cccccc", "#dddddd",
    ]

    sprite = gw.capture_sprite(0, 2, 0, 1)

    assert sprite == "1x2:000000aaaaaa"


def test_scale_maps_user_coordinates_to_canvas_pixels():
    gw = SpriteTestWindow(width=640, height=480)
    gw.set_scale(-10, 10, -10, 10)

    assert gw._transform_graphic(-10, -10) == (0, 479)
    assert gw._transform_graphic(10, 10) == (639, 0)
    assert gw._transform_graphic(0, 0) == (320, 240)


def test_scale_reset_restores_identity_mapping():
    gw = SpriteTestWindow(width=6, height=4)
    gw.set_scale(-1, 1, -1, 1)
    gw.reset_scale()

    assert gw._transform_graphic(0, 0) == (0, 3)
    assert gw._transform_graphic(5, 3) == (5, 0)


def test_scale_cursor_roundtrip_for_programmatic_moves():
    gw = ScaleCursorTestWindow(width=640, height=480)
    gw.set_scale(-10, 10, -10, 10)

    gw.move_cursor(3, -4)
    assert gw.get_cursor_position() == (3, -4)

    gw._set_cursor_from_canvas(0, 479)
    assert gw.get_cursor_position() == (-10, -10)


def test_scale_border_maps_limits_to_inset_pixels():
    gw = SpriteTestWindow(width=640, height=480)
    gw.set_scale(1960, 1990, 0, 200, 80)

    assert gw._transform_graphic(1960, 0) == (80, 399)
    assert gw._transform_graphic(1990, 200) == (559, 80)


def test_circle_radius_respects_scale_units():
    gw = ScaleCursorTestWindow(width=101, height=101)
    gw.set_scale(0, 10, 0, 10)

    gw.circle(5, 5, 1)

    sx, sy = gw._transform_graphic(6, 5)
    assert gw.buffer[gw._index(sx, sy)] != gw.background_color


def test_filled_circle_radius_respects_scale_units():
    gw = ScaleCursorTestWindow(width=101, height=101)
    gw.set_scale(0, 10, 0, 10)

    gw.filled_circle(5, 5, 1)

    sx, sy = gw._transform_graphic(6, 5)
    assert gw.buffer[gw._index(sx, sy)] != gw.background_color


def test_circle_i_f_draws_partial_arc():
    gw_full = ScaleCursorTestWindow(width=121, height=121)
    gw_full.circle(60, 60, 20)
    full_count = sum(1 for c in gw_full.buffer if c != gw_full.background_color)

    gw_arc = ScaleCursorTestWindow(width=121, height=121)
    gw_arc.circle(60, 60, 20, inicio=0, final=math.pi / 2)
    arc_count = sum(1 for c in gw_arc.buffer if c != gw_arc.background_color)

    assert 0 < arc_count < full_count


def test_fcircle_and_fcircler_support_i_f_with_mandatory_color_when_i_f_present():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=121, height=121)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("FCIRCLE 60,60,20,2,0,PI/2")
    sector_count = sum(1 for c in gw.buffer if c != gw.background_color)
    assert sector_count > 0

    gw_full = ScaleCursorTestWindow(width=121, height=121)
    interpreter.graphics_window = gw_full
    interpreter.execute_command("FCIRCLE 60,60,20")
    full_count = sum(1 for c in gw_full.buffer if c != gw_full.background_color)
    assert sector_count < full_count

    gw_rel = ScaleCursorTestWindow(width=121, height=121)
    interpreter.graphics_window = gw_rel
    interpreter.execute_command("MOVE 60,60")
    interpreter.execute_command("FCIRCLER 0,0,20,2,0,PI/2")
    rel_count = sum(1 for c in gw_rel.buffer if c != gw_rel.background_color)
    assert rel_count > 0

    with pytest.raises(ReturnMain):
        interpreter.execute_command("FCIRCLE 60,60,20,,0,PI/2")


def test_circle_family_interprets_fifth_parameter_as_aspect():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=140, height=140)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None
    interpreter.execute_command("MOVE 70,70")

    # 5º parámetro => asp (sin i/f). Debe dibujar elipses/círculos rellenos válidos.
    interpreter.execute_command("CIRCLE 70,70,20,2,2")
    sx, sy = gw._transform_graphic(110, 70)  # x + r*asp
    assert gw.buffer[gw._index(sx, sy)] != gw.background_color

    interpreter.execute_command("CIRCLER 0,0,20,2,2")
    sx, sy = gw._transform_graphic(110, 70)
    assert gw.buffer[gw._index(sx, sy)] != gw.background_color

    interpreter.execute_command("FCIRCLE 70,70,20,2,2")
    sx, sy = gw._transform_graphic(95, 70)
    assert gw.buffer[gw._index(sx, sy)] != gw.background_color

    interpreter.execute_command("MOVE 70,70")
    interpreter.execute_command("FCIRCLER 0,0,20,2,2")
    sx, sy = gw._transform_graphic(95, 70)
    assert gw.buffer[gw._index(sx, sy)] != gw.background_color


def test_rectangle_draw_outline_only():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=80, height=80)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("RECTANGLE 10,10,30,30")
    sx, sy = gw._transform_graphic(10, 10)
    assert gw.buffer[gw._index(sx, sy)] != gw.background_color
    sx, sy = gw._transform_graphic(20, 20)
    assert gw.buffer[gw._index(sx, sy)] == gw.background_color


def test_rectangle_clipping_does_not_create_artificial_edge():
    gw = ScaleCursorTestWindow(width=40, height=40)

    gw.rectangle(-5, 5, 5, 20)

    sx, sy = gw._transform_graphic(0, 12)
    assert gw.buffer[gw._index(sx, sy)] == gw.background_color
    sx, sy = gw._transform_graphic(5, 12)
    assert gw.buffer[gw._index(sx, sy)] != gw.background_color


def test_filled_shapes_commands():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=80, height=80)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("FRECTANGLE 10,10,30,30")
    sx, sy = gw._transform_graphic(20, 20)
    assert gw.buffer[gw._index(sx, sy)] != gw.background_color

    gw.buffer = [gw.background_color] * (gw.width * gw.height)
    interpreter.execute_command("FTRIANGLE 10,10,40,10,25,40")
    sx, sy = gw._transform_graphic(25, 20)
    assert gw.buffer[gw._index(sx, sy)] != gw.background_color


def test_triangle_draw_outline_only():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=80, height=80)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("TRIANGLE 10,10,40,10,25,40")
    sx, sy = gw._transform_graphic(25, 10)
    assert gw.buffer[gw._index(sx, sy)] != gw.background_color
    sx, sy = gw._transform_graphic(25, 20)
    assert gw.buffer[gw._index(sx, sy)] == gw.background_color


def test_scale_parameter_functions_fallback_to_physical_without_window():
    interpreter = BasicInterpreter()

    assert interpreter.evaluate_expression("XMIN") == 0
    assert interpreter.evaluate_expression("YMIN") == 0
    assert interpreter.evaluate_expression("BORDER") == 0
    assert interpreter.evaluate_expression("XMAX") == interpreter.evaluate_expression("WIDTH") - 1
    assert interpreter.evaluate_expression("YMAX") == interpreter.evaluate_expression("HEIGHT") - 1


def test_scale_parameter_functions_return_active_scale_values():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=800, height=600)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -PI,PI,-1,1,20")

    assert interpreter.evaluate_expression("XMIN") == pytest.approx(-math.pi)
    assert interpreter.evaluate_expression("XMAX") == pytest.approx(math.pi)
    assert interpreter.evaluate_expression("YMIN") == -1
    assert interpreter.evaluate_expression("YMAX") == 1
    assert interpreter.evaluate_expression("BORDER") == 20


def test_scale_activation_resets_origin_and_clipping():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=640, height=480)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    gw.set_origin(12, 34, 10, 300, 400, 20)
    assert gw.origin_x == 12
    assert gw.origin_y == 34

    interpreter.execute_command("SCALE -5,5,-2,2")

    assert gw.origin_x == 0
    assert gw.origin_y == 0
    assert gw.w_left == 0
    assert gw.w_top == 0
    assert gw.w_right == gw.width - 1
    assert gw.w_bottom == gw.height - 1


def test_origin_errors_when_scale_is_active():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=640, height=480)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -1,1,-1,1")

    with pytest.raises(ReturnMain):
        interpreter.execute_command("ORIGIN 10,20")

    assert gw.origin_x == 0
    assert gw.origin_y == 0


def test_origin_allowed_again_after_scale_reset():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=640, height=480)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -1,1,-1,1")
    interpreter.execute_command("SCALE")
    interpreter.execute_command("ORIGIN 10,20")

    assert gw.origin_x == 10
    assert gw.origin_y == 20


def test_graphics_commands_keep_fractional_coords_with_scale():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=800, height=600)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -PI,PI,-1,1")
    interpreter.execute_command("MOVE -PI,0")
    interpreter.execute_command("DRAW -1.57,-0.99")
    assert gw.get_cursor_position()[0] == pytest.approx(-1.57)
    assert gw.get_cursor_position()[1] == pytest.approx(-0.99)

    interpreter.execute_command("PLOT 0.25,0.75")
    assert gw.get_cursor_position()[0] == pytest.approx(0.25)
    assert gw.get_cursor_position()[1] == pytest.approx(0.75)


class AxisStubWindow:
    def __init__(self):
        self.closed = False
        self.last_x_axis = None
        self.last_y_axis = None
        self.last_x_axis_subdivisions = None
        self.last_y_axis_subdivisions = None
        self.scale_bounds = (-10, 10, -20, 20)
        self.scale_border = 0
        self.scale_explicit = False
        self.cross_at_x = None
        self.cross_at_y = None
        self.origin_reset = False

    def get_scale_bounds(self):
        return self.scale_bounds

    def get_scale_border(self):
        return self.scale_border

    def has_explicit_scale(self):
        return self.scale_explicit

    def set_scale(self, x_min, x_max, y_min, y_max, border=0):
        self.scale_bounds = (x_min, x_max, y_min, y_max)
        self.scale_border = border
        self.scale_explicit = True

    def reset_scale(self):
        self.scale_bounds = None
        self.scale_border = 0
        self.scale_explicit = False

    def reset_origin_and_clipping(self):
        self.origin_reset = True

    def set_cross_at(self, x_int=None, y_int=None):
        self.cross_at_x = x_int
        self.cross_at_y = y_int

    def x_axis(self, yint, tics, xmin, xmax, brdr, side, orie, tics_force_scientific=False, tic_subdivisions=1):
        self.last_x_axis = (yint, tics, xmin, xmax, brdr, side, orie)
        self.last_x_axis_subdivisions = tic_subdivisions

    def y_axis(self, xint, tics, ymin, ymax, brdr, side, tics_force_scientific=False, tic_subdivisions=1):
        self.last_y_axis = (xint, tics, ymin, ymax, brdr, side)
        self.last_y_axis_subdivisions = tic_subdivisions


def test_scale_command_dispatches_to_graphics_window():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -5,5,-2,2")
    assert gw.scale_bounds == (-5, 5, -2, 2)
    assert gw.scale_border == 0
    assert gw.scale_explicit is True

    interpreter.execute_command("SCALE -5,5,-2,2,80")
    assert gw.scale_bounds == (-5, 5, -2, 2)
    assert gw.scale_border == 80
    assert gw.scale_explicit is True

    interpreter.execute_command("SCALE")
    assert gw.scale_bounds is None
    assert gw.scale_border == 0
    assert gw.scale_explicit is False


def test_scale_off_is_not_supported_anymore():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    try:
        interpreter.execute_command("SCALE OFF")
    except Exception:
        pass

    assert gw.scale_bounds == (-10, 10, -20, 20)


def test_xaxis_yaxis_canonical_syntax_uses_scale_defaults_and_avl_options():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -10,10,-20,20")
    interpreter.execute_command("XAXIS")
    assert gw.last_x_axis == (0, 1, -10, 10, 0, "below", "horizontal")

    interpreter.execute_command("XAXIS 2,-3,3,1,1")
    assert gw.last_x_axis == (0, 2, -3, 3, 0, "above", "vertical")

    interpreter.execute_command("YAXIS 0.5")
    assert gw.last_y_axis == (0, 0.5, -20, 20, 0, "left")


def test_xaxis_yaxis_first_argument_is_tics():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE 0,100,-50,50")
    interpreter.execute_command("XAXIS 2,0,100,1,1")
    assert gw.last_x_axis == (0, 2, 0, 100, 0, "above", "vertical")

    interpreter.execute_command("YAXIS 5,-50,50,1")
    assert gw.last_y_axis == (0, 5, -50, 50, 0, "right")


def test_xaxis_yaxis_subdivisions_are_forwarded():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE 0,100,-50,50")
    interpreter.execute_command("XAXIS 2,0,100,1,1,5")
    assert gw.last_x_axis == (0, 2, 0, 100, 0, "above", "vertical")
    assert gw.last_x_axis_subdivisions == 5

    interpreter.execute_command("YAXIS 5,-50,50,1,4")
    assert gw.last_y_axis == (0, 5, -50, 50, 0, "right")
    assert gw.last_y_axis_subdivisions == 4


def test_xaxis_yaxis_invalid_subdivisions_are_rejected():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None
    interpreter.execute_command("SCALE -5,5,-2,2")

    with pytest.raises(ReturnMain):
        interpreter.execute_command("XAXIS 1,,,,,0")
    with pytest.raises(ReturnMain):
        interpreter.execute_command("XAXIS 1,,,,,2.5")
    with pytest.raises(ReturnMain):
        interpreter.execute_command("YAXIS 1,,,,-3")
    with pytest.raises(ReturnMain):
        interpreter.execute_command("YAXIS 1,,,,1.5")


def test_xaxis_yaxis_require_explicit_scale():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    with pytest.raises(ReturnMain):
        interpreter.execute_command("XAXIS")

    with pytest.raises(ReturnMain):
        interpreter.execute_command("YAXIS")


def test_crossat_sets_default_intercepts_for_axes():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -5,5,-2,2")
    interpreter.execute_command("CROSSAT 1.5,-0.25")
    interpreter.execute_command("XAXIS")
    assert gw.last_x_axis == (-0.25, 1, -5, 5, 0, "below", "horizontal")

    interpreter.execute_command("YAXIS")
    assert gw.last_y_axis == (1.5, 1, -2, 2, 0, "left")


def test_axis_optional_blanks_allow_skipping_params():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -5,5,-2,2")
    interpreter.execute_command("CROSSAT 2,3")

    interpreter.execute_command("XAXIS 2,,,,1")
    assert gw.last_x_axis == (3, 2, -5, 5, 0, "below", "vertical")

    interpreter.execute_command("YAXIS 2,,,-1")
    assert gw.last_y_axis == (2, 2, -2, 2, 0, "none")


def test_crossat_without_arguments_clears_custom_intercepts():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -5,5,-2,2")
    interpreter.execute_command("CROSSAT 2,3")
    interpreter.execute_command("CROSSAT")
    interpreter.execute_command("XAXIS")
    assert gw.last_x_axis == (0, 1, -5, 5, 0, "below", "horizontal")


def test_xaxis_yaxis_default_border_uses_active_scale_border():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -5,5,-2,2,80")

    interpreter.execute_command("XAXIS")
    assert gw.last_x_axis == (0, 1, -5, 5, 80, "below", "horizontal")

    interpreter.execute_command("YAXIS")
    assert gw.last_y_axis == (0, 1, -2, 2, 80, "left")


def test_xaxis_yaxis_explicit_border_legacy_syntax_is_rejected():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -5,5,-2,2,80")

    with pytest.raises(ReturnMain):
        interpreter.execute_command("XAXIS 0,1,-5,5,6")

    with pytest.raises(ReturnMain):
        interpreter.execute_command("YAXIS 0,1,-2,2,7")


def test_xaxis_yaxis_negative_side_disables_labels():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -5,5,-5,5")

    interpreter.execute_command("XAXIS 1,-5,5,-1,0")
    assert gw.last_x_axis == (0, 1, -5, 5, 0, "none", "horizontal")

    interpreter.execute_command("YAXIS 1,-5,5,-1")
    assert gw.last_y_axis == (0, 1, -5, 5, 0, "none")


def test_axis_legacy_leading_blank_for_removed_intercept_is_rejected():
    interpreter = BasicInterpreter()
    gw = AxisStubWindow()
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -5,5,-2,2")
    interpreter.execute_command("CROSSAT 2,3")

    with pytest.raises(ReturnMain):
        interpreter.execute_command("XAXIS ,2,,,,1")

    with pytest.raises(ReturnMain):
        interpreter.execute_command("YAXIS ,2,,,-1")


def test_axis_no_label_mode_draws_tics_without_gprint_calls():
    gw = AxisNoLabelWindow(width=200, height=120)

    gw.x_axis(0, 1, -5, 5, border=10, label_side='none', orientation='horizontal')
    gw.y_axis(0, 1, -3, 3, border=10, label_side='none')

    assert gw.gprint_calls == 0
    assert gw.dirty_grid.dirty_blocks


def test_axis_intersection_tick_draws_like_any_other_tick():
    gwx = ScaleCursorTestWindow(width=220, height=160)
    gwx.set_scale(-5, 5, -5, 5, border=10)
    bgx = gwx.background_color
    x0, y0 = gwx._transform_graphic(0, 0)
    gwx.x_axis(0, 1, -5, 5, border=10, label_side='none', orientation='horizontal')
    assert gwx.buffer[gwx._index(x0, y0 - 1)] != bgx

    gwy = ScaleCursorTestWindow(width=220, height=160)
    gwy.set_scale(-5, 5, -5, 5, border=10)
    bgy = gwy.background_color
    x1, y1 = gwy._transform_graphic(0, 0)
    gwy.y_axis(0, 1, -5, 5, border=10, label_side='none')
    assert gwy.buffer[gwy._index(x1 + 1, y1)] != bgy


def test_axis_subticks_are_drawn_when_spacing_is_enough():
    gw = ScaleCursorTestWindow(width=220, height=160)
    gw.set_scale(-5, 5, -5, 5, border=10)
    bg = gw.background_color
    x_sub, y_axis_px = gw._transform_graphic(1, 0)

    gw.x_axis(0, 2, -5, 5, border=10, label_side='none', orientation='horizontal', tic_subdivisions=4)

    assert gw.buffer[gw._index(x_sub, y_axis_px - 3)] != bg


def test_axis_subticks_are_skipped_when_too_dense():
    gw_base = ScaleCursorTestWindow(width=220, height=160)
    gw_base.set_scale(-5, 5, -5, 5, border=10)
    gw_base.x_axis(0, 2, -5, 5, border=10, label_side='none', orientation='horizontal', tic_subdivisions=1)

    gw_dense = ScaleCursorTestWindow(width=220, height=160)
    gw_dense.set_scale(-5, 5, -5, 5, border=10)
    gw_dense.x_axis(0, 2, -5, 5, border=10, label_side='none', orientation='horizontal', tic_subdivisions=8)

    assert gw_dense.buffer == gw_base.buffer


def test_axis_labels_always_render_with_smallfont_even_if_current_font_is_big():
    gw = AxisLabelFontWindow(width=220, height=140)
    gw.bitmap_font = big_bitmap_font

    gw.x_axis(0, 1, -5, 5, border=10, label_side='below', orientation='horizontal')
    gw.y_axis(0, 1, -3, 3, border=10, label_side='left')

    assert gw.fonts_seen
    assert all(font is small_bitmap_font for font in gw.fonts_seen)
    assert gw.bitmap_font is big_bitmap_font


def test_yaxis_huge_range_keeps_tick_generation_bounded():
    gw = AxisLabelFontWindow(width=320, height=220)
    gw.set_scale(-128, 128, 0, 1.8e27, border=40)

    # Simula el caso real: primero XAXIS y luego YAXIS.
    gw.x_axis(0, 32, -128, 128, border=40, label_side='below', orientation='horizontal')
    gw.fonts_seen.clear()
    gw.y_axis(0, 1e5, 0, 1.8e27, border=40, label_side='left')

    assert gw.dirty_grid.dirty_blocks
    # Con densidad excesiva, los tics/etiquetas se omiten por completo.
    assert gw.fonts_seen == []


def test_axis_skips_ticks_when_too_dense():
    gw = AxisLabelTextWindow(width=320, height=220)
    gw.set_scale(-10, 10, -10, 10, border=20)

    # 0.1 unidades en un rango de 20 sobre ~280 px => ~1.4 px entre tics (<18 px con etiquetas).
    gw.x_axis(0, 0.1, -10, 10, border=20, label_side='below', orientation='horizontal')
    gw.y_axis(0, 0.1, -10, 10, border=20, label_side='left')

    # Se dibuja el eje, pero sin tics ni rótulos por densidad excesiva.
    assert gw.dirty_grid.dirty_blocks
    assert gw.labels_seen == []


def test_axis_uses_scientific_labels_when_tics_literal_is_exponential():
    interpreter = BasicInterpreter()
    gw = AxisLabelTextWindow(width=320, height=220)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -10,10,0,1000000,20")
    interpreter.execute_command("YAXIS 1E+5")

    assert gw.labels_seen
    assert any('E' in label for label in gw.labels_seen)


def test_axis_segments_respect_explicit_min_max_ranges():
    interpreter = BasicInterpreter()
    gw = ScaleCursorTestWindow(width=800, height=600)
    interpreter.graphics_window = gw
    interpreter.ensure_graphics_window = lambda: None

    interpreter.execute_command("SCALE -10,2,-10,2,50")
    interpreter.execute_command("XAXIS -1,-8.5,0")
    interpreter.execute_command("YAXIS -1,-8.5,0")

    def color_at(x, y):
        sx, sy = gw._transform_graphic(x, y)
        return gw.buffer[gw._index(sx, sy)]

    bg = gw.background_color

    # Solo debe existir tramo horizontal en x in [-8.5, 0] para y=0
    assert color_at(-4, 0) != bg
    assert color_at(-9, 0) == bg
    assert color_at(1, 0) == bg

    # Solo debe existir tramo vertical en y in [-8.5, 0] para x=0
    assert color_at(0, -4) != bg
    assert color_at(0, -9) == bg
    assert color_at(0, 1) == bg


def test_axes_without_scale_keep_full_span_even_with_origin():
    gw = ScaleCursorTestWindow(width=300, height=200)
    gw.set_origin(100, 60)

    gw.x_axis(0, 1, -10, 10, border=20, label_side='below', orientation='horizontal')
    gw.y_axis(0, 1, -10, 10, border=20, label_side='left')

    bg = gw.background_color
    _, y_axis_px = gw._transform_graphic(0, 0)
    x_axis_px, _ = gw._transform_graphic(0, 0)

    # Tramo completo útil (comportamiento clásico sin SCALE activo)
    assert gw.buffer[gw._index(20, y_axis_px)] != bg
    assert gw.buffer[gw._index(279, y_axis_px)] != bg
    assert gw.buffer[gw._index(x_axis_px, 20)] != bg
    assert gw.buffer[gw._index(x_axis_px, 179)] != bg


@pytest.mark.parametrize(
    ("expr", "expected"),
    [
        ('0=0', -1),
        ('0=1', 0),
        ('0<>0', 0),
        ('0<>1', -1),
        ('1<2', -1),
        ('2<1', 0),
        ('2<=2', -1),
        ('3<=2', 0),
        ('5>4', -1),
        ('4>5', 0),
        ('5>=5', -1),
        ('4>=5', 0),
        ('"A"="A"', -1),
        ('"A"="B"', 0),
        ('"A"<>"B"', -1),
        ('1<2<3', -1),
    ],
)
def test_evaluate_expression_relational_operators_produce_basic_booleans(expr, expected):
    interpreter = BasicInterpreter()
    assert interpreter.evaluate_expression(expr) == expected


@pytest.mark.parametrize(
    ("expr", "expected"),
    [
        ('-(0=0)', 1),
        ('-(0=1)', 0),
        ('1+(0=0)', 0),
        ('2*(1=1)', -2),
        ('ABS(0=0)', 1),
        ('ABS(1=0)', 0),
        ('INT(0=0)', -1),
        ('NOT(1=1)', 0),
        ('NOT(1=0)', -1),
        ('(1<2) AND (2<3)', -1),
        ('(1<2) AND (2>3)', 0),
        ('(1<2) OR (2>3)', -1),
        ('(1<2) XOR (2>3)', -1),
        ('(1<2) XOR (2<3)', 0),
        ('NOT((1<2) AND (2<3))', 0),
        ('INT((1<2) OR (2>3))', -1),
    ],
)
def test_evaluate_expression_propagates_basic_booleans(expr, expected):
    interpreter = BasicInterpreter()
    assert interpreter.evaluate_expression(expr) == expected


def _make_mouse_test_interpreter(line_number=100):
    interpreter = BasicInterpreter()
    interpreter.program = {line_number: {'code': 'RETURN'}}
    interpreter.line_numbers = [line_number]
    interpreter._line_to_index = {line_number: 0}
    return interpreter


class _MouseCursorWindowStub:
    def __init__(self):
        self.closed = False
        self.calls = []

    def set_mouse_cursor_visible(self, visible):
        self.calls.append(bool(visible))


class _ScreenCloseWindowStub:
    def __init__(self):
        self.closed = False
        self.calls = []

    def reset_graphics_window(self):
        self.calls.append("reset")

    def on_close(self):
        self.calls.append("close")
        self.closed = True


def test_on_mouse_registers_and_clears_handlers():
    interpreter = _make_mouse_test_interpreter()

    interpreter.execute_command('ON MOUSE MOVE GOSUB 100')
    assert interpreter.mouse_handlers['MOVE'] == 100
    assert interpreter._mouse_enabled is True

    interpreter.execute_command('ON MOUSE MOVE GOSUB 0')
    assert 'MOVE' not in interpreter.mouse_handlers
    assert interpreter._mouse_enabled is False


def test_handle_mouse_event_updates_state_and_queue_only_when_running():
    interpreter = _make_mouse_test_interpreter()
    interpreter.execute_command('ON MOUSE MOVE GOSUB 100')

    interpreter.running = True
    interpreter.stopped = False
    interpreter.handle_mouse_event('MOVE', 5, 6, left_override=True)

    assert interpreter._mouse_state['x'] == 5
    assert interpreter._mouse_state['y'] == 6
    assert interpreter._mouse_state['left'] == 1
    assert len(interpreter._mouse_event_queue) == 1

    interpreter.running = False
    interpreter.handle_mouse_event('MOVE', 7, 8, left_override=False)

    assert interpreter._mouse_state['x'] == 7
    assert interpreter._mouse_state['y'] == 8
    assert interpreter._mouse_state['left'] == 0
    assert len(interpreter._mouse_event_queue) == 1  # no events queued when not running


def test_poll_interrupts_dispatches_mouse_events():
    interpreter = _make_mouse_test_interpreter()
    interpreter.program[10] = {'code': 'RETURN'}
    interpreter.line_numbers = [10, 100]
    interpreter._line_to_index = {10: 0, 100: 1}

    interpreter.execute_command('ON MOUSE MOVE GOSUB 100')
    interpreter.running = True
    interpreter.stopped = False
    interpreter.current_line = 0
    interpreter.current_command_index = 0

    interpreter.handle_mouse_event('MOVE', 1, 2)

    assert interpreter._poll_interrupts() is True
    assert interpreter.gosub_stack
    assert interpreter.current_line == 0


def test_mouse_query_functions_return_latest_state():
    interpreter = _make_mouse_test_interpreter()
    interpreter.handle_mouse_event('MOVE', 12, 34, left_override=True, right_override=False)

    assert interpreter._mousex([]) == '12'
    assert interpreter._mousey([]) == '34'
    assert interpreter._mouseleft([]) == '-1'
    assert interpreter._mouseright([]) == '0'
    assert interpreter._mouseevent([]) == '"MOVE"'


def test_mouse_command_toggles_cursor_visibility():
    interpreter = _make_mouse_test_interpreter()
    stub = _MouseCursorWindowStub()
    interpreter.graphics_window = stub

    interpreter.execute_command('MOUSE 0')
    assert interpreter._mouse_cursor_visible_requested is False
    assert stub.calls[-1] is False

    interpreter.execute_command('MOUSE ON')
    assert interpreter._mouse_cursor_visible_requested is True
    assert stub.calls[-1] is True


def test_mouse_command_rejects_invalid_visibility_value():
    interpreter = _make_mouse_test_interpreter()
    with pytest.raises(ReturnMain):
        interpreter.execute_command('MOUSE 2')


def test_screen_close_releases_graphics_window_reference():
    interpreter = _make_mouse_test_interpreter()
    stub = _ScreenCloseWindowStub()
    interpreter.graphics_window = stub

    interpreter.execute_command('SCREEN CLOSE')

    assert stub.calls == ["reset", "close"]
    assert interpreter.graphics_window is None


def test_tk_event_to_key_codes_maps_special_and_alpha_keys():
    left_evt = SimpleNamespace(char="", keysym="Left")
    assert _tk_event_to_key_codes(left_evt) == {28}

    a_evt = SimpleNamespace(char="a", keysym="a")
    assert _tk_event_to_key_codes(a_evt) == {65, 97}


def test_keydown_reports_current_key_state(monkeypatch):
    interpreter = _make_mouse_test_interpreter()
    monkeypatch.setattr("basic._scan_gui_once", lambda *_args, **_kwargs: 0)
    interpreter.graphics_window = SimpleNamespace(closed=False, root=None)
    interpreter._set_keys_down({28})

    assert interpreter.evaluate_expression("KEYDOWN(28)") == -1
    assert interpreter.evaluate_expression("KEYDOWN(29)") == 0


def test_keydown_rejects_invalid_code():
    interpreter = _make_mouse_test_interpreter()
    with pytest.raises(ReturnMain):
        interpreter.evaluate_expression("KEYDOWN(-1)")
    with pytest.raises(ReturnMain):
        interpreter.evaluate_expression("KEYDOWN(1.5)")


def test_inkey_prefilled_queue_returns_character_instead_of_variable_name():
    interpreter = _make_mouse_test_interpreter()
    _key_q.clear()
    _key_q.append(ord("A"))
    try:
        assert interpreter.evaluate_expression("INKEY$") == "A"
    finally:
        _key_q.clear()


def test_inkey_handles_quote_char_and_ctrl_c_from_prefilled_queue():
    interpreter = _make_mouse_test_interpreter()
    _key_q.clear()
    _key_q.append(34)  # '"'
    _key_q.append(3)   # Ctrl-C residual -> ignored as empty
    try:
        assert interpreter.evaluate_expression("INKEY$") == '"'
        assert interpreter.evaluate_expression("INKEY$") == ""
    finally:
        _key_q.clear()


def test_gui_special_keysym_mapping_for_inkey_contains_arrow_keys():
    assert _GUI_KEYSYM_TO_CODE["Left"] == 28
    assert _GUI_KEYSYM_TO_CODE["Right"] == 29
    assert _GUI_KEYSYM_TO_CODE["Up"] == 30
    assert _GUI_KEYSYM_TO_CODE["Down"] == 31


def test_resume_next_advances_to_next_line_inside_multiline_function(run_basic_interpreter):
    commands = [
        'NEW',
        '1 ON ERROR GOTO 100',
        '10 DEF FNF(X,Y)',
        '20 LET FNF=X',
        '30 IF Y<X THEN 50',
        '35 LET FNF=X/0',
        '40 LET FNF=Y',
        '50 FNEND',
        '60 PRINT FNF(2,9)',
        '65 END',
        '100 RESUME NEXT',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 100. Error while handling errors.' not in output
    assert ' 9' in output


def test_resume_next_advances_to_next_statement_inside_multiline_function(run_basic_interpreter):
    commands = [
        'NEW',
        '10 ON ERROR GOTO 100',
        '20 DEF FNF(X)',
        '30 LET FNF=1/X : LET FNF=9',
        '40 FNEND',
        '50 PRINT FNF(0)',
        '60 END',
        '100 RESUME NEXT',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Error in error handler' not in output
    assert 'Division by zero.' not in output
    assert ' 9' in output


def test_on_error_resume_next_shorthand_advances_inside_multiline_function(run_basic_interpreter):
    commands = [
        'NEW',
        '1 ON ERROR RESUME NEXT',
        '10 DEF FNF(X,Y)',
        '20 LET FNF=X',
        '30 IF Y<X THEN 50',
        '35 LET FNF=X/0',
        '40 LET FNF=Y',
        '50 FNEND',
        '60 PRINT FNF(2,9)',
        '65 END',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Division by zero.' not in output
    assert 'Error in error handler' not in output
    assert ' 9' in output


def test_on_error_resume_next_shorthand_advances_to_next_statement_inside_multiline_function(run_basic_interpreter):
    commands = [
        'NEW',
        '10 ON ERROR RESUME NEXT',
        '20 DEF FNF(X)',
        '30 LET FNF=1/X : LET FNF=9',
        '40 FNEND',
        '50 PRINT FNF(0)',
        '60 END',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Division by zero.' not in output
    assert 'Error in error handler' not in output
    assert ' 9' in output


def test_on_error_goto_rejects_multiline_function_body_as_handler_target(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF FNF(X)',
        '20 FNF=X',
        '30 FNEND',
        '40 ON ERROR GOTO 20',
        '50 X=1/0',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 40. Invalid target line.' in output


def test_resume_rejects_multiline_function_body_as_target(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF FNF(X)',
        '20 FNF=X',
        '30 FNEND',
        '40 ON ERROR GOTO 100',
        '50 X=1/0',
        '60 END',
        '100 RESUME 20',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Error in error handler: Invalid target line.' in output


def test_resume_to_same_multiline_function_body_line_remains_valid(run_basic_interpreter):
    commands = [
        'NEW',
        '100 ON ERROR GOTO 210',
        '110 DEF FNFACT(N)',
        '120 K=0 : R=1',
        '130 IF N<=1 THEN R=R/K',
        '135 GOTO 170',
        '140 FOR I=1 TO N',
        '150 R=R*I',
        '160 NEXT',
        '170 FNFACT=R',
        '180 FNEND',
        '190 PRINT FNFACT(1)',
        '200 END',
        '210 K=1 : R=19 : RESUME 170',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert ' 19' in output


def test_tron_traces_error_handler_inside_multiline_function(run_basic_interpreter):
    commands = [
        'NEW',
        '1 ON ERROR GOTO 100',
        '10 DEF FNF(X,Y)',
        '20 LET FNF=X',
        '30 IF Y<X THEN 50',
        '35 LET FNF=X/0',
        '40 LET FNF=Y',
        '50 FNEND',
        '60 PRINT FNF(2,9)',
        '65 END',
        '100 RESUME NEXT',
        'TRON',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert '[1][10][60][20][30][35][100][40][50] 9' in output
    assert '[65]' in output


def test_multiline_function_plain_assignment_cannot_return_array(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF FNZ(A)',
        '20 A(12)=8',
        '25 FNZ=A',
        '30 FNEND',
        '40 DIM Z(12),V(12)',
        '50 MAT V=FNZ(Z)',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 25. Invalid value type.' in output


def test_single_line_function_cannot_accept_array_argument(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF FNR(X)=X(0)',
        '20 DIM A(1),B(1)',
        '30 MAT A=CON',
        '40 MAT B=FNR(A)',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 40. Invalid value type.' in output


def test_single_line_function_cannot_return_array(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DIM A(1,1),B(1,1)',
        '20 MAT A=CON',
        '30 DEF FNR()=A',
        '40 MAT B=FNR()',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 40. Invalid value type.' in output


def test_mat_assignment_rejects_three_dimensional_array(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DIM A(1,2,3),B(1,2,3)',
        '20 A(1,2,3)=7',
        '30 MAT A=B',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 30. Invalid number of dimensions.' in output


def test_multiline_function_mat_return_rejects_three_dimensional_array(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF FNR(A)',
        '20 MAT FNR=A',
        '30 FNEND',
        '40 DIM A(1,2,3),B(1,2,3)',
        '50 A(1,2,3)=7',
        '60 MAT B=FNR(A)',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 60. Invalid number of dimensions.' in output


def test_multiline_function_array_argument_is_local_copy(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF FNCOPY(A)',
        '20 A(1)=9',
        '30 FNCOPY=A(1)',
        '40 FNEND',
        '50 DIM Z(2)',
        '60 Z(1)=3',
        '70 PRINT FNCOPY(Z)',
        '80 PRINT Z(1)',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert ' 9' in output
    assert ' 3' in output


def test_multiline_function_redim_on_array_argument_is_local(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF FNRESIZE(A)',
        '20 REDIM A(5)',
        '30 A(5)=7',
        '40 FNRESIZE=UBOUND(A)',
        '50 FNEND',
        '60 DIM Z(2)',
        '70 PRINT FNRESIZE(Z)',
        '80 PRINT UBOUND(Z)',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert ' 5' in output
    assert ' 2' in output


def test_multiline_function_local_scalars_hide_globals_and_restore_them(run_basic_interpreter):
    commands = [
        'NEW',
        '10 A=7:B$="OUT"',
        '20 DEF FNF(X)',
        '30 LOCAL A,B$',
        '40 A=A+X',
        '50 B$="IN"',
        '60 FNF=A',
        '70 FNEND',
        '80 PRINT FNF(3)',
        '90 PRINT A',
        '100 PRINT B$',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert '\n 3\n 7\nOUT\n' in output


def test_multiline_function_local_array_does_not_modify_global_array(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DIM D(2)',
        '20 D(1)=4',
        '30 DEF FNF()',
        '40 LOCAL D(3)',
        '50 D(1)=9',
        '60 FNF=D(1)',
        '70 FNEND',
        '80 PRINT FNF()',
        '90 PRINT D(1)',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert '\n 9\n 4\n' in output


def test_multiline_subroutine_local_declarations_coexist_with_reference_parameters(run_basic_interpreter):
    commands = [
        'NEW',
        '10 A=1',
        '20 DIM D(2),Z(2)',
        '30 D(1)=4:Z(1)=1',
        '40 DEF SUB WORK(T)',
        '50 LOCAL A,D(3)',
        '60 A=7',
        '70 D(1)=9',
        '80 T(1)=5',
        '90 PRINT A',
        '100 PRINT D(1)',
        '110 SUBEND',
        '120 CALL WORK(Z)',
        '130 PRINT A',
        '140 PRINT D(1)',
        '150 PRINT Z(1)',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert '\n 7\n 9\n 1\n 4\n 5\n' in output


def test_local_must_appear_in_initial_block_of_multiline_function(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF FNF(X)',
        '20 A=X',
        '30 LOCAL B',
        '40 FNF=A',
        '50 FNEND',
        '60 PRINT FNF(2)',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 30. LOCAL must appear in the initial block of a multiline function or subroutine.' in output


def test_local_must_appear_in_initial_block_of_multiline_subroutine(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF SUB WORK(X)',
        '20 PRINT X',
        '30 LOCAL B',
        '40 SUBEND',
        '50 CALL WORK(2)',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 30. LOCAL must appear in the initial block of a multiline function or subroutine.' in output


def test_multiline_subroutine_passes_scalars_by_value(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF SUB INC(X)',
        '20 X=X+1',
        '30 PRINT X',
        '40 SUBEND',
        '50 A=4',
        '60 CALL INC(A)',
        '70 PRINT A',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert ' 5' in output
    assert ' 4' in output


def test_multiline_subroutine_passes_arrays_by_reference_and_restores_sub_name(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF SUB TOUCH(A)',
        '20 TOUCH=7',
        '30 A(1)=9',
        '40 PRINT TOUCH',
        '50 SUBEND',
        '60 TOUCH=2',
        '70 DIM Z(2)',
        '80 Z(1)=3',
        '90 CALL TOUCH(Z)',
        '100 PRINT TOUCH',
        '110 PRINT Z(1)',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert '\n 7\n 2\n 9\n' in output


def test_multiline_subroutine_subexit_stops_execution_early(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF SUB EARLY()',
        '20 PRINT 1',
        '30 SUBEXIT',
        '40 PRINT 2',
        '50 SUBEND',
        '60 CALL EARLY()',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert '\n 1\n' in output
    assert '\n 2\n' not in output


def test_on_error_goto_rejects_multiline_subroutine_body_as_handler_target(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF SUB T()',
        '20 PRINT 1',
        '30 SUBEND',
        '40 ON ERROR GOTO 20',
        '50 X=1/0',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 40. Invalid target line.' in output


def test_goto_rejects_multiline_subroutine_body_from_outside(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF SUB T()',
        '20 PRINT 1',
        '30 SUBEND',
        '40 GOTO 20',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 40. Invalid target line.' in output


def test_resume_next_advances_inside_multiline_subroutine(run_basic_interpreter):
    commands = [
        'NEW',
        '1 ON ERROR GOTO 100',
        '10 DEF SUB SHOW(X,Y)',
        '20 PRINT X',
        '30 IF Y<X THEN 50',
        '35 A=X/0',
        '40 PRINT Y',
        '50 SUBEND',
        '60 CALL SHOW(2,9)',
        '65 END',
        '100 RESUME NEXT',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 100. Error while handling errors.' not in output
    assert ' 2' in output
    assert ' 9' in output


def test_call_is_not_allowed_inside_multiline_function(run_basic_interpreter):
    commands = [
        'NEW',
        '10 DEF FNF()',
        '20 CALL SHOW()',
        '30 FNF=1',
        '40 FNEND',
        '50 DEF SUB SHOW()',
        '60 SUBEND',
        '70 PRINT FNF()',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert 'Line 20. Instruction not allowed inside a function.' in output


def test_syntax_highlight_uses_keyword_style_for_subroutine_names():
    highlighted_def = syntax_highlight('10 DEF SUB POINT(X)')
    highlighted_call = syntax_highlight('20 CALL POINT()')

    keyword_name = f'{KEYWORD_STYLE}POINT{RESET}'
    variable_name = f'{VARIABLE_STYLE}POINT{RESET}'

    assert keyword_name in highlighted_def
    assert keyword_name in highlighted_call
    assert variable_name not in highlighted_def
    assert variable_name not in highlighted_call


def test_tron_traces_error_handler_inside_multiline_subroutine(run_basic_interpreter):
    commands = [
        'NEW',
        '1 ON ERROR GOTO 100',
        '10 DEF SUB SHOW(X,Y)',
        '20 PRINT X',
        '30 IF Y<X THEN 50',
        '35 A=X/0',
        '40 PRINT Y',
        '50 SUBEND',
        '60 CALL SHOW(2,9)',
        '65 END',
        '100 RESUME NEXT',
        'TRON',
        'RUN',
        'EXIT',
    ]

    output = run_basic_interpreter(commands)
    assert '[1][10][60][20] 2' in output
    assert '[30][35][100][40] 9' in output
    assert '[50][65]' in output
