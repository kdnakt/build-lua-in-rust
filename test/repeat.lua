repeat
    print "hello, repeat"
    local ok = true
    if true then
        continue
    end
    print "should not print this!"
until ok
