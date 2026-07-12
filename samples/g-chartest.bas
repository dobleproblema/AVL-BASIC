100 CLG : LOCATE 0,1
110 FOR X=2 TO 1 STEP -1
115 IF X=2 THEN SMALLFONT ELSE BIGFONT
120 INK 1 : DISP SPC(X);"gajo,pera, niño. ÑO.ÑO, Ñoño"
140 DISP SPC(X);"ABCDEFGHIJKLMNÑOPQRSTUVWXYZ"
160 DISP SPC(X);"abcdefghijklmnñopqrstuvwxyz"
180 DISP SPC(X);"0123456789ºª";CHR$(34);" ÂâÊêÎîÔôÛûĂăĚě"
200 DISP SPC(X);"¡!¿?|@#·$€£~%&/\()='^*+[]{}-_.:,;<>"
220 DISP SPC(X);"aáeéiíoóuúüAÁEÉIÍOÓUÚÜÀÈèÌìÒòÙù"
240 DISP SPC(X);"ÄäÅå Ææ Ëë Ïï ÖöØø Œœ ẞß ÝýŸÿ ÃãÕõ çÇ"
260 DISP SPC(X);"ÐðÞþ ŐőŰű"
280 DISP SPC(X);"4-~7*2=-+1+8/\7^2_9>3<4"
300 DISP SPC(X);"¿Aa?¡Bb! 25€ 12$ a&B,(332}-(hello]"
320 DISP SPC(X);"LA LUNA SE PONÍA BAJO EL CIELO AÑIL"
340 DISP SPC(X);"La 4ª se ponía bajo el 5º"
350 IF X=2 THEN SMALLFONT OPAQUE ELSE BIGFONT OPAQUE
360 INK "yellow" : PAPER "green" : DISP SPC(X);"La luna se ponía bajo el cielo añil"
370 PAPER 0 : IF X=2 THEN SMALLFONT TRANSPARENT ELSE BIGFONT TRANSPARENT
380 INK "yellow" : DISP SPC(X);"HOLA"; : LOCATE X,VPOS : INK "red" : DISP "____"
410 NEXT
