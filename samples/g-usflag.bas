100 REM Drawing the U.S. flag with LOCATE + DISP
110 x=13 : y=8 : m=2 : s$=" *   *   *   *   *   *   * "
120 MODE 640
130 FOR i=y TO y+12
140   'Print the stripes alternating colors
150   LOCATE x,i : DISP SPACE$(53),0,m : m=3-m
160   'Print 7 lines of stars alternating between 6 and 5
170   IF i<y+7 THEN LOCATE x,i : DISP MID$(s$,m+m-1,23),1,4
180 NEXT i
190 BIGFONT : LOCATE 15,25 : DISP "OLD GLORY"
200 END

