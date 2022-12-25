// HELPERJS provides a few of helper functions to bridge the Javascript and Rust worlds.

HELPERJS = {
  STORAGE: [], // A place to store JS objects so they can be referenced by their index from Rust.
  EMPTY_SLOTS: [], // List of slots in STORAGE that have been freed and can be reused.

  // Store an object in this.STORAGE and return the slot number.
  storeObject: function(obj) {
      var idx = this.EMPTY_SLOTS.pop();
      if (undefined == idx) { idx = this.STORAGE.length };
      this.STORAGE[idx] = obj;
      return idx;
  },

  // Load an object from this.STORAGE. 
  loadObject: function(idx) {
      return this.STORAGE[idx];
  },

  // Release a slot in this.STORAGE.
  releaseObject: function(idx) {
      delete this.STORAGE[idx];
      this.EMPTY_SLOTS.push(idx);
  },

  // Copy a Javascript string to Emscripten memory.
  // Returns a pointer in the Emscripten heap that points to
  // the number of UTF-16 characters in the string, encoded as
  // a uint32, followed by the UTF-16 encoded string.
  copyStringToHeap: function(string) {
      var char_count = string.length;
      var buf = Module._malloc(4 + char_count*2);
      Module.HEAPU32[buf / 4] = char_count;
      for (var idx=0; idx < char_count; idx++) {
          Module.HEAPU16[2 + buf/2 + idx] = string.charCodeAt(idx);
      }
      return buf;
  },

  // Copy a UTF-16 encoded string from Emscripten heap
  // into a Javascript string.
  copyStringFromHeap: function(ptr, size) {
      var string = "";
      var offset = ptr / 2;
      for (var idx = 0; idx < size; idx++) {
          string = string.concat(String.fromCharCode(Module.HEAPU16[offset+idx]));
      }
      Module._free(ptr);
      return string;
  }
};