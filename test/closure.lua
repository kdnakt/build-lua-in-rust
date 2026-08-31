local function newCounter()
    local i = 0
    return function()
        i = i + 1
        print(i)
    end
end

local c = newCounter()
c()
c()
