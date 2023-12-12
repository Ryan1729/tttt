--
-- Place this and customized socket.lua into bizhawk's Lua folder, and 
-- start this script in the lua console. Note this will stop running in
-- bizhawk if you send a line consiting of "break" (no quotes)
-- You can connect by running `telnet localhost 44444`, for example.
-- 
local DEBUG_MODE = true

console.clear()
console.log("Script started up.")
console.log("DEBUG_MODE = " .. (DEBUG_MODE and "true" or "false"))

-- load namespace
local socket = require("socket")
-- create a TCP socket and bind it to the local host, at any port
--local server = assert(socket.bind("*", 0))
-- create a TCP socket and bind it to the local host, at a good port
local server = assert(socket.bind("*", 44444))
-- find out which port the OS chose for us
local ip, port = server:getsockname()
-- print a message informing what's up
print("Please telnet to localhost on port " .. port)

local SOCKET_TIMEOUT_S = 1.0/120.0

server:settimeout(SOCKET_TIMEOUT_S)

local PRINT_TIMEOUT_IN_FRAMES = 300
local onscreen = {}

function screen_print(line)
  local last_entry = onscreen[#onscreen]
  if last_entry and last_entry.line == line then
    last_entry.frames = PRINT_TIMEOUT_IN_FRAMES
    return
  end

  table.insert(onscreen, { line = line, frames = PRINT_TIMEOUT_IN_FRAMES})
  
  if DEBUG_MODE then
	print(line)
  end
end

function frame_end()
  local x = 0
  local y = 0
  for _, entry in pairs(onscreen) do
    gui.drawText(x, y, entry.line)
    y = y + 16
    
    entry.frames = entry.frames - 1
  end
  
  for i = #onscreen, 1, -1 do
    if onscreen[i].frames <= 0 then
      table.remove(onscreen, i)
    end
  end
  
  emu.frameadvance()
end

function clear_screen_prints()
  -- Apparently the fastest way to clear an array
  -- https://stackoverflow.com/a/30815687
  local count = #onscreen
  for i=0, count do
    onscreen[i]=nil
  end
end

local client = nil

-- loop until we get a client
while 1 do
  -- wait for a connection from any client
  local client_local, err = server:accept()
  
  if not err then 
	-- clear any leftover errors
    clear_screen_prints()
	
	screen_print("accepted connection")
  
	client = client_local
  
    break
  end
  
  screen_print("accept error:" .. err)
  frame_end()
end

-- make sure we don't block too long
client:settimeout(SOCKET_TIMEOUT_S)

-- loop forever waiting for messages
while 1 do
  -- receive the line
  local line, err = client:receive()
  
  if err then
	if err ~= 'timeout' then
	  screen_print("receive error:" .. err)
	end
	
	frame_end()
  else
    -- if there was no error, send it back to the client
    screen_print("got:" .. line)
    client:send(line .. "\n") 

    frame_end()

    if line == "break" then
      break
    end
  end
end

-- clear any leftover errors
clear_screen_prints()

-- done with client, close the object
client:close()

screen_print("broke out of receive loop")
while #onscreen > 0 do
  frame_end()
end

