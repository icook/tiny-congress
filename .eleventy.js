module.exports = function(eleventyConfig) {
  // Copy the css directory to the output
  eleventyConfig.addPassthroughCopy("src/css");

  // Add date filter
  eleventyConfig.addFilter("date", function(date, format) {
    const d = new Date(date instanceof Date ? date : Date.now());
    if (format === "Y") {
      return d.getFullYear().toString();
    }
    return d.toLocaleDateString();
  });

  return {
    dir: {
      input: "src",
      output: "_site"
    }
  };
};
