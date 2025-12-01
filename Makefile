.PHONY: all download whisper test-rs


# must make the statment before .env, or the eq always evaluate
ifeq ($(url),)
	$(error url is not supplied, Please specify it like this: make url="https://...")
endif

include ./src-tauri/.env

YT := ./src-tauri/binaries/ytdown-aarch64-apple-darwin
WHISPER := ./src-tauri/binaries/whisper-aarch64-apple-darwin

dev:
	@bun tauri dev

build:
	@bun tauri build


all: download split

download:
	$(YT) --proxy $(PROXY_URL) --force-overwrites  -x -f "worstaudio[ext=webm]" --extract-audio --audio-format wav --postprocessor-args "-ar 16000 -ac 1" -o /Users/bruce/Library/Caches/newscenter1/temp.wav $(url)
	 
split:
	ffmpeg -y -i cache/temp.wav -f segment -segment_time 00:10:00 -reset_timestamps 1 -c copy "cache/temp_%03d.wav"  

# use api from groq which is much faster
whisper:
	$(WHISPER) -m ../llama-rust-desktop/src-tauri/resources/models/medium.bin -f "./cache/temp.wav" -l auto -otxt

# use api from groq
summary:
	$(eval PROMPT := "Summarize the following content: ")
	$(eval PROMPT += $(shell cat cache/temp.wav.txt))
	@echo $(PROMPT)
# llama-cli.exe -m models/new3/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf --repeat-penalty 1.1 --ctx-size 8196 -ngl 99 --simple-io --in-prefix "<|eot_id|><|start_header_id|>user<|end_header_id|>\n\n" --in-suffix "<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n" -p "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\nYou are a helpful, smart, kind, and efficient AI assistant." -e --multiline-input --no-display-prompt -f cache/temp.wav.txt
#
test:
	$(YT) --proxy $(PROXY_URL) --skip-download --write-subs --sub-lang en -o 'cache/test.%(ext)s' $(url)

check:
	$(YT) --proxy $(PROXY_URL) --list-subs $(url)

title:
	$(YT) --proxy $(PROXY_URL) --skip-download --print "%(title)s|%(upload_date)s" $(url)

test-rs:
	cargo test -p tube-rs
