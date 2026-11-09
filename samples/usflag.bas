100 REM Console version of g-usflag.bas, with the same 53 by 13 proportions.
110 REM Build each row left to right with COLOR(fg,bg).
120 S$=" *   *   *   *   *   *   * "
130 W=53 : H=13 : CW=23 : CH=7
140 FOR R=1 TO H
150   Stripe=IIF(R MOD 2=1,2,1)
160   IF R<=CH THEN
170     Stars$=MID$(S$,1+2*((R-1) MOD 2),CW)
180     PRINT TAB(3);COLOR(1,4);Stars$;COLOR(1,Stripe);SPACE$(W-CW)
190   ELSE
200     PRINT TAB(3);COLOR(1,Stripe);SPACE$(W)
210   END IF
220 NEXT R
230 PRINT TAB(3+INT((W-9)/2));"OLD GLORY"
240 PRINT TAB(3);"Stars ";COLOR(1,4);USING "0#";39;
250 PRINT " Stripes ";COLOR(0,1);"1";COLOR(1,2);"3"
260 END
