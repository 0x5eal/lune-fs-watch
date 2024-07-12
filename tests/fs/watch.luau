local fs = require("@lune/fs")
local task = require("@lune/task")
local utils = require("utils")

local TEMP_DIR_PATH = "bin/"
local TEMP_ROOT_PATH = TEMP_DIR_PATH .. "fs_watch_test"

fs.writeDir(TEMP_ROOT_PATH)

-- local watcherThread = coroutine.create(fs.watch)
-- coroutine.resume(watcherThread, TEMP_ROOT_PATH, "**/*")

local function makeArmHandler(tab)
	return function(paths)
		for _, path in paths do
			table.insert(tab, path)
		end
	end
end

local addedFiles, readFiles, removedFiles, changedFiles, renamedFiles = {}, {}, {}, {}, {}
local watcherThread = coroutine.create(function()
	fs.watch(TEMP_ROOT_PATH, "**/*.{json*,bin}", {
		added = makeArmHandler(addedFiles),
		read = makeArmHandler(readFiles),
		removed = makeArmHandler(removedFiles),
		changed = makeArmHandler(changedFiles),
		renamed = makeArmHandler(renamedFiles),
	})
end)
coroutine.resume(watcherThread)

fs.writeFile(TEMP_ROOT_PATH .. "/file.bin", utils.binaryBlob)
fs.writeFile(TEMP_ROOT_PATH .. "/file.json", utils.jsonBlob)

fs.move(TEMP_ROOT_PATH .. "/file.json", TEMP_ROOT_PATH .. "/file.jsonc")

fs.removeFile(TEMP_ROOT_PATH .. "/file.bin")

-- TODO: Test editing a file
local jsoncFilePath = TEMP_ROOT_PATH .. "/file.jsonc"
fs.writeFile(jsoncFilePath, "// This is a comment\n" .. fs.readFile(jsoncFilePath))
fs.removeFile(jsoncFilePath)
task.wait(5)
coroutine.close(watcherThread)
fs.removeDir(TEMP_ROOT_PATH)
print("addedFiles: ", addedFiles)
print("readFiles: ", readFiles)
print("removedFiles: ", removedFiles)
print("changedFiles: ", changedFiles)
print("renamedFiles: ", renamedFiles)
